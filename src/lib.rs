use std::task::{Poll, Context};
use std::future::Future;
use std::marker::{PhantomData, PhantomPinned};
use std::pin::{Pin};
use std::sync::{Arc, Mutex};
use axum_core::extract::{FromRequestParts};
use futures_core::future::BoxFuture;
use tower_service::Service;
use http::{StatusCode};
use http::request::Parts;
use tower_layer::Layer;

pub trait Guard {
    fn check_guard(&self, expected:&Self) -> bool;
}

impl<State,G> GuardService< State,G>
    where
        State:Clone,
        G: Clone + FromRequestParts<State, Rejection = StatusCode> + Guard {
    pub fn new(state:State,expected_guard:G) -> GuardService< State,G> {
        Self{ state, expected_guard}
    }
}
#[derive(Clone)]
pub struct GuardService<State,G>
    where
    State:Clone,
    G:Clone{
    state:State,
    expected_guard:G,
}


impl<State,G> Service<Parts> for GuardService<State,G>
    where
    State: Sync + Send + Clone + 'static,
    G: Clone + FromRequestParts<State, Rejection = StatusCode> + Guard + Sync + Send + 'static, {
    type Response = GuardServiceResponse;
    type Error = StatusCode;
    type Future = BoxFuture<'static,Result<Self::Response,Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut req: Parts) -> Self::Future {
        let expected = self.expected_guard.clone();
        let state = self.state.clone();
        Box::pin(async move {
                let result = match G::from_request_parts(&mut req, &state).await {
                    Ok(guard) => {
                        guard.check_guard(&expected)
                    },
                    Err(status) => {
                        return Err(status);
                    }
                };
            Ok(GuardServiceResponse(result,req))
        })
    }
}

#[derive(Clone)]
pub struct AndGuardService<S1,S2>
    where
    S1: Service<Parts, Response=GuardServiceResponse,Error=StatusCode> + Send + Clone + 'static,
    <S1 as Service<Parts>>::Future: Send,
    S2: Service<Parts, Response=GuardServiceResponse,Error=StatusCode> + Send + Clone + 'static,
    <S2 as Service<Parts>>::Future: Send,{
    left:S1,
    right:S2,
}
impl<S1,S2> AndGuardService<S1,S2>
    where
        S1: Service<Parts, Response=GuardServiceResponse,Error=StatusCode> + Send + Clone + 'static,
        <S1 as Service<Parts>>::Future: Send,
        S2: Service<Parts, Response=GuardServiceResponse,Error=StatusCode> + Send + Clone + 'static,
        <S2 as Service<Parts>>::Future: Send, {
    pub fn new(left:S1,right:S2) -> Self{
        Self{ left, right }
    }
}
impl<S1,S2> Service<Parts> for AndGuardService<S1,S2>
    where
        S1: Service<Parts, Response=GuardServiceResponse,Error=StatusCode> + Send + Clone + 'static,
        <S1 as Service<Parts>>::Future: Send,
        S2: Service<Parts, Response=GuardServiceResponse,Error=StatusCode> + Send + Clone + 'static,
        <S2 as Service<Parts>>::Future: Send, {
    type Response = GuardServiceResponse;
    type Error = StatusCode;
    type Future = BoxFuture<'static,Result<Self::Response,Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, parts: Parts) -> Self::Future {
        let mut left = self.left.clone();
        let mut right = self.right.clone();
        Box::pin(async move {
            let GuardServiceResponse(result,parts) =
                left.call(parts).await?;
            if result{
                right.call(parts).await
            } else {
                Ok(GuardServiceResponse(false, parts, ))
            }
        })

    }
}
#[derive(Clone)]
pub struct OrGuardService<S1,S2>
    where
        S1: Service<Parts, Response=GuardServiceResponse,Error=StatusCode> + Send + Clone + 'static,
        <S1 as Service<Parts>>::Future: Send,
        S2: Service<Parts, Response=GuardServiceResponse,Error=StatusCode> + Send + Clone + 'static,
        <S2 as Service<Parts>>::Future: Send,{
    left:S1,
    right:S2,
}
impl<S1,S2> OrGuardService<S1,S2>
    where
        S1: Service<Parts, Response=GuardServiceResponse,Error=StatusCode> + Send + Clone + 'static,
        <S1 as Service<Parts>>::Future: Send,
        S2: Service<Parts, Response=GuardServiceResponse,Error=StatusCode> + Send + Clone + 'static,
        <S2 as Service<Parts>>::Future: Send, {
    pub fn new(left:S1,right:S2) -> Self{
        Self{ left, right }
    }
}
impl<S1,S2> Service<Parts> for OrGuardService<S1,S2>
    where
        S1: Service<Parts, Response=GuardServiceResponse,Error=StatusCode> + Send + Clone + 'static,
        <S1 as Service<Parts>>::Future: Send,
        S2: Service<Parts, Response=GuardServiceResponse,Error=StatusCode> + Send + Clone + 'static,
        <S2 as Service<Parts>>::Future: Send, {
    type Response = GuardServiceResponse;
    type Error = StatusCode;
    type Future = BoxFuture<'static,Result<Self::Response,Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, parts: Parts) -> Self::Future {
        let mut left = self.left.clone();
        let mut right = self.right.clone();
        Box::pin(async move {
            let GuardServiceResponse(result,parts) =
                left.call(parts).await?;
            if result{
                Ok(GuardServiceResponse(true, parts))
            } else {
                right.call(parts).await
            }
        })

    }
}


pub struct GuardServiceResponse( bool, Parts);



#[cfg(test)]
pub mod tests {
    use tokio::time::{sleep, Duration};
    use axum::body::Body;
    use axum::error_handling::{HandleError, HandleErrorLayer};
    use axum::handler::Handler;
    use axum::Router;
    use axum::routing::get;
    use http::{HeaderValue, Request, StatusCode};
    use tower::util::ServiceExt;
    use axum::BoxError;
    use axum::extract::State;
    use axum::middleware::Next;
    use axum_core::extract::FromRequestParts;
    use tower::{service_fn, ServiceBuilder};

    #[derive(Clone, Debug, PartialEq)]
    pub struct ArbitraryData {
        data: String,
    }

    impl Guard for ArbitraryData {
        fn check_guard(&self, expected: &Self) -> bool {
            *self == *expected
        }
    }

    #[async_trait::async_trait]
    impl FromRequestParts<ArbitraryData> for ArbitraryData {
        type Rejection = StatusCode;

        async fn from_request_parts(parts: &mut Parts, state: &ArbitraryData) -> Result<Self, Self::Rejection> {
            Ok(Self {
                data: parts.headers.get(state.data.clone())
                    .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
                    .to_str()
                    .map_err(|err| StatusCode::INTERNAL_SERVER_ERROR)?
                    .to_string()
            })
        }
    }

    use super::*;

    #[tokio::test]
    async fn test_guard_service_ok() {
        let (mut parts, _) = Request::new(()).into_parts();
        parts.headers.insert(
            "data",
            HeaderValue::from_static("other_data"));
        assert!(GuardService::new(
            ArbitraryData { data: "data".into() },
            ArbitraryData { data: "other_data".into() })
            .call(parts).await.unwrap().0);
    }

    #[tokio::test]
    async fn test_guard_service_not_from_request_parts_error() {
        let (mut parts, _) = Request::new(()).into_parts();
        parts.headers.insert(
            "BIG-UH-OH",
            HeaderValue::from_static("other_data"));
        let result = GuardService::new(
            ArbitraryData { data: "data".into() },
            ArbitraryData { data: "other_data".into() })
            .call(parts).await;
        assert_eq!(result.err(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    }

    #[tokio::test]
    async fn test_guard_service_expected_failed() {
        let (mut parts, _) = Request::new(()).into_parts();
        parts.headers.insert(
            "data",
            HeaderValue::from_static("other_data"));
        assert!(!(GuardService::new(
            ArbitraryData { data: "data".into() },
            ArbitraryData { data: "NOT OTHER DATA MY BAD".into() })
            .call(parts).await.unwrap().0));
    }

    #[tokio::test]
    async fn test_and() {
        let (mut parts, _) = Request::new(()).into_parts();
        parts.headers.insert(
            "data",
            HeaderValue::from_static("data"));
        parts.headers.insert(
            "other_data",
            HeaderValue::from_static("other_data"));
        let data = ArbitraryData { data: "data".into() };
        let other_data = ArbitraryData { data: "data".into() };
        assert!(
            AndGuardService::new(
                GuardService::new(
                    data.clone(), data.clone()
                ),
                GuardService::new(
                    other_data.clone(), other_data.clone()
                )
            ).call(parts).await.unwrap().0
        )
    }

    #[tokio::test]
    async fn test_or() {
        let (mut parts, _) = Request::new(()).into_parts();
        parts.headers.insert(
            "data",
            HeaderValue::from_static("data"));
        parts.headers.insert(
            "other_data",
            HeaderValue::from_static("NUH UH BRUH NUHHHHH"));
        let data = ArbitraryData { data: "data".into() };
        let other_data = ArbitraryData { data: "data".into() };
        assert!(
            OrGuardService::new(
                GuardService::new(
                    data.clone(), data.clone()
                ),
                GuardService::new(
                    other_data.clone(), other_data.clone()
                )
            ).call(parts).await.unwrap().0
        )
    }

    #[tokio::test]
    async fn test_and_deep() {
        let (mut parts, _) = Request::new(()).into_parts();
        parts.headers.insert(
            "data",
            HeaderValue::from_static("data"));
        let data = ArbitraryData { data: "data".into() };
        assert!(
            AndGuardService::new(
                AndGuardService::new(
                    AndGuardService::new(
                        GuardService::new(
                            data.clone(), data.clone()
                        ),
                        GuardService::new(
                            data.clone(), data.clone()
                        ),
                    ),
                    AndGuardService::new(
                        GuardService::new(
                            data.clone(), data.clone()
                        ),
                        GuardService::new(
                            data.clone(), data.clone()
                        )
                    )
                ),
                AndGuardService::new(
                    AndGuardService::new(
                        GuardService::new(
                            data.clone(), data.clone()
                        ),
                        GuardService::new(
                            data.clone(), data.clone()
                        ),
                    ),
                    AndGuardService::new(
                        GuardService::new(
                            data.clone(), data.clone()
                        ),
                        GuardService::new(
                            data.clone(), data.clone()
                        )
                    )
                )
            ).call(parts).await.unwrap().0
        )
    }
}
