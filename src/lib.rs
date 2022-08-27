#![feature(pin_macro)]
#![feature(async_closure)]

use std::fmt::Debug;
use std::future::Future;
use std::marker::PhantomData;
use std::mem;
use std::pin::{Pin, pin};
use std::process::Output;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, ready};
use axum_core::extract::{FromRequestParts};
use axum_core::response::{IntoResponse, Response};
use tower_layer::Layer;
use tower_service::Service;
use http::{Request, StatusCode};
use async_trait::async_trait;
use std::ops::DerefMut;
use http::request::Parts;
use axum::extract::TypedHeader;
use axum::RequestPartsExt;
use futures_core::future::BoxFuture;
/*
    .layer(GuardLayer::with(A{a_data}.and(B{b_data}.or(C{c_data}))))
    = And(A,Or(B,C))
*/


/// Implement Guard for a `Type{data:Data}` that implements FromRequestParts.State/// Then create a `GuardLayer::with(Type{data:Data{...})`
/// layered on the route you want to protect.
/// The data given inside `GuardLayer::with()` will then be the
/// expected data you write your `check_guard()` method for.
/// ```
/// use axum_guard_combinator::Guard;
///
/// #[derive(Clone,Debug,PartialEq)]
///     pub struct ArbitraryData{
///         data:String,
///     }
///
/// impl Guard for ArbitraryData{
///         fn check_guard(&self, expected: &Self) -> bool {
///             *self == *expected
///         }
///     }
///
/// ```
pub trait Guard {
    fn check_guard(&self, expected:&Self) -> bool;
}

pub trait GuardExt: Guard + Sized + Clone + FromRequestParts<()> {
    fn and<Right: 'static + Guard + Clone + FromRequestParts<()>>(self, other: Right)
        -> And<Self, Right> {
        And(self, other)
    }

    fn or<Right: 'static + Guard + Clone + FromRequestParts<()>>(self, other: Right)
        -> Or<Self, Right> {
        Or(self, other)
    }
}
impl<T: Guard + Clone + FromRequestParts<()>> GuardExt for T {}

pub struct And<Left,Right>(Left, Right) where
    Left:'static + Guard + Clone + FromRequestParts<()>,
    Right: 'static + Guard + Clone + FromRequestParts<()>;

impl<Left,Right> Clone for And<Left,Right>
    where
        Left:'static + Guard + Clone + FromRequestParts<()>,
        Right: 'static + Guard + Clone + FromRequestParts<()> {
    fn clone(&self) -> Self {
        Self(self.0.clone(),self.1.clone())
    }
}

impl<Left,Right> Guard for And<Left,Right>where
    Left:'static + Guard + Clone + FromRequestParts<()>,
    Right: 'static + Guard + Clone + FromRequestParts<()> {
    fn check_guard(&self, expected: &Self) -> bool {
        self.0.check_guard(&expected.0) && self.1.check_guard(&expected.1)
    }
}

#[async_trait::async_trait]
impl<Left,Right,State> FromRequestParts<State> for And<Left,Right>
    where
        State:Send+Sync,
        Left:'static + Guard + Clone + FromRequestParts<()> + Send,
        Right: 'static + Guard + Clone + FromRequestParts<()> + Send{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &State) -> Result<Self, Self::Rejection> {
        let left = parts.extract::<Left>().await
            .map_err(|err|StatusCode::INTERNAL_SERVER_ERROR)?;
        let right = parts.extract::<Right>().await
            .map_err(|err|StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Self(left,right))
    }
}


pub struct Or<Left,Right>(Left, Right) where
    Left:'static + Guard + Clone + FromRequestParts<()>,
    Right: 'static + Guard + Clone + FromRequestParts<()>;

impl<Left,Right> Clone for Or<Left,Right>
    where
        Left:'static + Guard + Clone + FromRequestParts<()>,
        Right: 'static + Guard + Clone + FromRequestParts<()> {
    fn clone(&self) -> Self {
        Self(self.0.clone(),self.1.clone())
    }
}

impl<Left, Right> Guard for Or<Left, Right> where
    Left:'static + Guard + Clone + FromRequestParts<()>,
    Right: 'static + Guard + Clone + FromRequestParts<()> {
    fn check_guard(&self, expected: &Self) -> bool {
        self.0.check_guard(&expected.0) || self.1.check_guard(&expected.1)
    }
}

#[async_trait::async_trait]
impl<Left,Right,State> FromRequestParts<State> for Or<Left,Right>
    where
        State:Send+Sync,
        Left:'static + Guard + Clone + FromRequestParts<()> + Send,
        Right: 'static + Guard + Clone + FromRequestParts<()> + Send{
    type Rejection = StatusCode;


    async fn from_request_parts(parts: &mut Parts, state: &State) -> Result<Self, Self::Rejection> {
        let left = parts.extract::<Left>().await
            .map_err(|err|StatusCode::INTERNAL_SERVER_ERROR)?;
        let right = parts.extract::<Right>().await
            .map_err(|err|StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Self(left,right))
    }
}

pub struct GuardLayer<G,B> {
    expected_guard: Option<G>,
    _marker: PhantomData<B>,
}

impl<G,B> GuardLayer<G,B>{
    pub fn with(expected_guard:G) -> Self {
        Self{
            expected_guard:Some(expected_guard),
            _marker: PhantomData
        }
    }
}

impl<S,B,G> Layer<S> for GuardLayer<G,B>
    where
        S: Service<Request<B>> + Clone + Send,
        B: Send + Sync,
        G: Guard + FromRequestParts<()> + Send + Sync + Clone{
    type Service = GuardService<S,G,B>;

    fn layer(&self, inner: S) -> Self::Service {
        GuardService{
            expected_guard: self.expected_guard.clone(),
            inner,
            _marker: PhantomData
        }
    }
}

pub struct GuardService<S,G,B>
    where
        S: Service<Request<B>> + Clone + Send,
        B: Send + Sync,
        G: Guard + FromRequestParts<()> + Send + Sync + Clone {
    expected_guard:Option<G>,
    inner:S,
    _marker:PhantomData<B>,
}
impl<S,G,B> Clone for GuardService<S,G,B>
    where
        S: Service<Request<B>> + Clone + Send,
        B: Send + Sync,
        G: Guard + FromRequestParts<()> + Send + Sync + Clone{
    fn clone(&self) -> Self {
        Self{
            expected_guard: self.expected_guard.clone(),
            inner: self.inner.clone(),
            _marker: PhantomData
        }
    }
}

impl<G,S,ResBody,B> Service<Request<B>> for GuardService<S,G,B>
    where
        ResBody:Default,
        S: Service<Request<B>, Response = Response<ResBody>> + Clone + Send + 'static,
        <S as Service<Request<B>>>::Future:Send,
        G: Guard + FromRequestParts<()> + Sync + Send + Clone + 'static,
        B: Send + Sync + 'static, {
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static,Result<Response<ResBody>,S::Error>>;

    fn poll_ready(&mut self, ctx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(ctx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let expected = self.expected_guard.take().unwrap();
        let clone = self.inner.clone();
        let mut inner = mem::replace(&mut self.inner, clone);
        Box::pin(
        async move {
            let (mut parts,body) = req.into_parts();
            let guard : G = match
                G::from_request_parts(&mut parts,&()).await {
                Ok(guard) => guard,
                Err(_) => {
                    let mut res = Response::new(ResBody::default());
                    *res.status_mut() = StatusCode::BAD_REQUEST;
                    return Ok(res);
                }
            };
            if guard.check_guard(&expected) {
                let req = Request::from_parts(parts,body);
                inner.call(req).await
            } else {
                let mut res = Response::new(ResBody::default());
                *res.status_mut() = StatusCode::UNAUTHORIZED;
                return Ok(res);
            }
        })
    }
}



#[cfg(test)]
pub mod tests {
    use tokio::time::{sleep,Duration};
    use axum::body::Body;
    use axum::error_handling::{HandleError, HandleErrorLayer};
    use axum::handler::Handler;
    use axum::Router;
    use axum::routing::get;
    use http::StatusCode;
    use tower::util::ServiceExt;
    use axum::BoxError;
    use axum::middleware::Next;
    use axum_core::extract::FromRequestParts;
    use tower::{service_fn, ServiceBuilder};

    use super::*;

    #[derive(Clone,Debug,PartialEq)]
    pub struct ArbitraryData{
        data:String,
    }
    impl Guard for ArbitraryData{
        fn check_guard(&self, expected: &Self) -> bool {
            *self == *expected
        }
    }
    #[async_trait::async_trait]
    impl<State:Send+Sync> FromRequestParts<State> for ArbitraryData {
        type Rejection = ();

        async fn from_request_parts(parts: &mut Parts, state: &State) -> Result<Self, Self::Rejection> {
            Ok(Self{
                data: parts.headers.get("data").unwrap()
                    .to_str().unwrap().to_string()
            })
        }
    }
    #[derive(Clone,Copy,Debug,PartialEq)]
    pub struct Always;

    impl Guard for Always {
        fn check_guard(&self,_:&Self) -> bool {
            true
        }
    }
    #[async_trait::async_trait]
    impl<State:Send+Sync> FromRequestParts<State> for Always {
        type Rejection = ();

        async fn from_request_parts(parts: &mut Parts, state: &State) -> Result<Self, Self::Rejection> {
            Ok(Self)
        }
    }
    #[derive(Clone,Copy,Debug,PartialEq)]
    pub struct Never;
    impl Guard for Never {
        fn check_guard(&self, expected: &Self) -> bool {
            false
        }
    }
    #[async_trait::async_trait]
    impl<State:Send+Sync> FromRequestParts<State> for Never {
        type Rejection = ();

        async fn from_request_parts(parts: &mut Parts, state: &State) -> Result<Self, Self::Rejection> {
            Ok(Self)
        }
    }
    async fn ok() -> StatusCode { StatusCode::OK }

    #[tokio::test]
    async fn test_always() {
        let app = Router::new()
            .route("/",get(ok))
            .layer(GuardLayer::with(Always));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_never() {
        let app = Router::new()
            .route("/",get(ok))
            .layer(GuardLayer::with(Never));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    #[tokio::test]
    async fn test_and_happy_path() {
        let app = Router::new()
            .route("/",get(ok)
                .layer(GuardLayer::with(Always.and(Always))));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
    #[tokio::test]
    async fn test_and_sad_path() {
        let app = Router::new()
            .route("/",get(ok)
                .layer(GuardLayer::with(Always.and(Never))));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    #[tokio::test]
    async fn test_or_happy_path() {
        let app = Router::new()
            .route("/",get(ok)
                .layer(GuardLayer::with(Always.or(Never))));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
    #[tokio::test]
    async fn test_or_sad_path() {
        let app = Router::new()
            .route("/",get(ok)
                .layer(GuardLayer::with(Never.or(Never))));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    #[tokio::test]
    async fn test_happy_nested() {
        let app = Router::new()
            .route("/",get(ok)
                .layer(GuardLayer::with(
                        Never.or(
                    Always.and(
                            Always.or(
                                Never))))));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
    #[tokio::test]
    async fn test_or_happy_path_with_data() {
        let app = Router::new()
            .route("/",get(ok)
                .layer(GuardLayer::with(ArbitraryData{
                    data:String::from("Hello World.")
                })));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("data","Hello World.")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
    #[tokio::test]
    async fn test_duplicate_layers_with_data() {
        let layers = ServiceBuilder::new()
            .layer(GuardLayer::with(ArbitraryData{
                data:String::from("Hello World.")
            }))
            .layer(
                GuardLayer::with(ArbitraryData{
                    data:String::from("Should fail.")}));
        let app = Router::new()
            .route("/",get(ok))
            .layer(layers);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("data","Hello World.")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        println!("{:?}",response.status());
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    async fn time_time<B>(req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
        sleep(Duration::from_millis(10)).await;
        Ok(next.run(req).await)
    }


    // My attempt at writing my own future panicked on inner service polling
    // I used BoxFuture.
    #[tokio::test]
    async fn test_happy_with_layered_polls() {
        let app = Router::new()
            .route("/",get(ok))
            .layer(axum::middleware::from_fn(time_time))
            .layer(GuardLayer::with(Always))
            .layer(axum::middleware::from_fn(time_time))
            .layer(tower_http::timeout::TimeoutLayer::new(Duration::from_secs(1)));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

}