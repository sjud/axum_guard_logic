use std::convert::Infallible;
use std::task::{Poll, Context};
use std::future::Future;
use std::marker::{PhantomData, PhantomPinned};
use std::pin::{Pin};
use std::sync::{Arc, Mutex};
use axum_core::body::BoxBody;
use axum_core::extract::{FromRequestParts};
use axum_core::response::{IntoResponse, Response};
use futures_core::future::BoxFuture;
use tower_service::Service;
use http::{Request, StatusCode};
use http::request::Parts;
use axum_core::BoxError;
use futures_core::ready;
use tower_layer::Layer;

pub trait Guard {
    fn check_guard(&self, expected:&Self) -> bool;
}

pub trait GuardServiceExt : Service<Parts, Response=GuardServiceResponse,Error=(StatusCode,String)> + Clone + Send {
    fn or<Other>(self,other:Other)
                 -> OrGuardService<Self,Other>
        where
            Other:Service<Parts, Response=GuardServiceResponse,Error=(StatusCode,String)> + Clone + Send,
            <Other as Service<Parts>>::Future: Send,
            <Self as Service<Parts>>::Future: Send {
        OrGuardService{
            left:self,
            right: other,
        }
    }
    fn and<Other>(self,other:Other)
                  -> AndGuardService<Self,Other>
        where
            Other:Service<Parts, Response=GuardServiceResponse,Error=(StatusCode,String)> + Clone + Send,
            <Other as Service<Parts>>::Future: Send,
            <Self as Service<Parts>>::Future: Send {
        AndGuardService{
            left:self,
            right: other,
        }
    }
    fn into_layer<ReqBody>(self) -> GuardLayer<Self,ReqBody> {
        GuardLayer::with(self)
    }
}
impl<T> GuardServiceExt for T
    where
        T: Service<Parts, Response=GuardServiceResponse,Error=(StatusCode,String)> + Clone + Send {}

pub struct GuardLayer<GuardService,ReqBody>
    where
        GuardService:Service<Parts, Response=GuardServiceResponse,Error=(StatusCode,String)>
        + Send + Clone + 'static {
    guard_service:GuardService,
    _marker:PhantomData<ReqBody>
}

impl<GuardService,ReqBody> Clone for GuardLayer<GuardService,ReqBody>
    where
        GuardService:Service<Parts, Response=GuardServiceResponse,Error=(StatusCode,String)>
        + Send + Clone + 'static {
    fn clone(&self) -> Self {
        Self{
            guard_service: self.guard_service.clone(),
            _marker: PhantomData,
        }
    }
}
impl<GuardService,ReqBody> GuardLayer<GuardService,ReqBody>
    where
        GuardService:Service<Parts, Response=GuardServiceResponse,Error=(StatusCode,String)>
        + Send + Clone + 'static {
    pub fn with(guard:GuardService) -> Self {
        Self{ guard_service:guard, _marker:PhantomData }
    }
}
impl<S,GuardService,ReqBody> Layer<S> for GuardLayer<GuardService,ReqBody>
    where
        S:Service<Request<ReqBody>> + Clone,
        GuardService:Service<Parts, Response=GuardServiceResponse,Error=(StatusCode,String)>
        + Send + Clone + 'static {
    type Service = GuardServiceWrapper<S,GuardService,ReqBody>;

    fn layer(&self, inner: S) -> Self::Service {
        GuardServiceWrapper{
            inner,
            guard_service:self.guard_service.clone(),
            _marker:PhantomData
        }
    }
}
pub struct GuardServiceWrapper<S,GuardService,ReqBody>
    where
        S:Service<Request<ReqBody>> + Clone,
        GuardService:Service<Parts, Response=GuardServiceResponse>
        + Send + Clone + 'static {
    inner:S,
    guard_service:GuardService,
    _marker:PhantomData<ReqBody>
}
impl<S,GuardService,ReqBody> Clone for GuardServiceWrapper<S,GuardService,ReqBody>
    where
        S:Service<Request<ReqBody>> + Clone,
        GuardService:Service<Parts, Response=GuardServiceResponse>
        + Send + Clone + 'static {
    fn clone(&self) -> Self {
        Self{
            inner: self.inner.clone(),
            guard_service: self.guard_service.clone(),
            _marker: PhantomData
        }
    }
}
impl<S,ReqBody,GuardService> Service<Request<ReqBody>> for
GuardServiceWrapper<S,GuardService,ReqBody>
    where
        ReqBody:Send +'static,
        S:Service<Request<ReqBody>, Response = Response, Error = Infallible> + Send + Clone + 'static,
        <S as Service<Request<ReqBody>>>::Future: Send,
        <GuardService as Service<Parts>>::Future: Send,
        GuardService:Service<Parts, Response=GuardServiceResponse, Error = (StatusCode,String)>
        + Send + Clone + 'static{
    type Response = Response;
    type Error = Infallible;
    type Future = BoxFuture<'static,Result<Response,Infallible>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let (parts,body) = req.into_parts();
        let f = self.guard_service.call(parts);
        let mut inner = self.inner.clone();
        Box::pin(async move {
            match f.await {
                Ok(GuardServiceResponse(result,parts)) => {
                    if result.0 {
                        inner.call(Request::from_parts(parts,body))
                            .await
                    } else {
                        Ok((StatusCode::UNAUTHORIZED,result.1.unwrap_or_default()).into_response())
                    }
                },
                Err(status) => {
                    Ok(status.into_response())
                }
            }
        })
    }
}
/*
This panics when the inner future returns pending... how to fix?
#[pin_project::pin_project]
pub struct GuardFuture<S,ReqBody,GuardService>
    where
        ReqBody:Send,
        S:Service<Request<ReqBody>, Response = Response, Error = Infallible> + Send + Clone,
        <S as Service<Request<ReqBody>>>::Future:Unpin,
        GuardService:Service<Parts, Response=GuardServiceResponse>
        + Send + Clone + 'static, {
    #[pin]
    f:<GuardService as Service<Parts>>::Future,
    inner:S,
    body:ReqBody
}

impl<S,ReqBody,GuardService> Future for GuardFuture<S,ReqBody,GuardService>
    where
        ReqBody:Send,
        S:Service<Request<ReqBody>, Response = Response, Error = Infallible> + Send + Clone,
        <S as Service<Request<ReqBody>>>::Future: Unpin,
        GuardService:Service<Parts, Response=GuardServiceResponse, Error = StatusCode>
        + Send + Clone + 'static, {
    type Output = Result<Response,Infallible>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        match ready!(this.f.poll(cx)) {
            Ok(GuardServiceResponse(result,parts)) => {
                if result {
                    Pin::new(&mut this.inner
                        .call(Request::from_parts(parts,this.body.take().unwrap())))
                        .poll(cx)
                } else {
                    Poll::Ready(Ok(StatusCode::UNAUTHORIZED.into_response()))
                }
            },
            Err(status) => {
                Poll::Ready(Ok(status.into_response()))
            }
        }
    }
}
*/
impl<State,G> GuardService< State,G>
    where
        State:Clone,
        G: Clone + FromRequestParts<State, Rejection = (StatusCode,String)> + Guard {
    pub fn new(state:State,expected_guard:G,err_msg:&'static str) -> GuardService< State,G> {
        Self{ state, expected_guard,err_msg}
    }
}
#[derive(Clone)]
pub struct GuardService<State,G>
    where
        State:Clone,
        G:Clone{
    state:State,
    expected_guard:G,
    err_msg:&'static str,
}


impl<State,G> Service<Parts> for GuardService<State,G>
    where
        State: Sync + Send + Clone + 'static,
        G: Clone + FromRequestParts<State, Rejection = (StatusCode,String)> + Guard + Sync + Send + 'static, {
    type Response = GuardServiceResponse;
    type Error = (StatusCode,String);
    type Future = BoxFuture<'static,Result<Self::Response,Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut req: Parts) -> Self::Future {
        let expected = self.expected_guard.clone();
        let state = self.state.clone();
        let err_msg = self.err_msg;
        Box::pin(async move {
            let result = match G::from_request_parts(&mut req, &state).await {
                Ok(guard) => {
                    guard.check_guard(&expected)
                },
                Err(status) => {
                    return Err(status);
                }
            };
            let err_msg = if !result {
                Some(String::from(err_msg))
            } else {None};
            Ok(GuardServiceResponse((result,err_msg),req))
        })
    }
}

#[derive(Clone)]
pub struct AndGuardService<S1,S2>
    where
        S1: Service<Parts, Response=GuardServiceResponse,Error=(StatusCode,String)> + Send + Clone + 'static,
        <S1 as Service<Parts>>::Future: Send,
        S2: Service<Parts, Response=GuardServiceResponse,Error=(StatusCode,String)> + Send + Clone + 'static,
        <S2 as Service<Parts>>::Future: Send,{
    left:S1,
    right:S2,
}
impl<S1,S2> AndGuardService<S1,S2>
    where
        S1: Service<Parts, Response=GuardServiceResponse,Error=(StatusCode,String)> + Send + Clone + 'static,
        <S1 as Service<Parts>>::Future: Send,
        S2: Service<Parts, Response=GuardServiceResponse,Error=(StatusCode,String)> + Send + Clone + 'static,
        <S2 as Service<Parts>>::Future: Send, {
    pub fn new(left:S1,right:S2) -> Self{
        Self{ left, right }
    }
}
impl<S1,S2> Service<Parts> for AndGuardService<S1,S2>
    where
        S1: Service<Parts, Response=GuardServiceResponse,Error=(StatusCode,String)> + Send + Clone + 'static,
        <S1 as Service<Parts>>::Future: Send,
        S2: Service<Parts, Response=GuardServiceResponse,Error=(StatusCode,String)> + Send + Clone + 'static,
        <S2 as Service<Parts>>::Future: Send, {
    type Response = GuardServiceResponse;
    type Error = (StatusCode,String);
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
            if result.0{
                right.call(parts).await
            } else {
                Ok(GuardServiceResponse((false,result.1), parts, ))
            }
        })

    }
}
#[derive(Clone)]
pub struct OrGuardService<S1,S2>
    where
        S1: Service<Parts, Response=GuardServiceResponse,Error=(StatusCode,String)> + Send + Clone + 'static,
        <S1 as Service<Parts>>::Future: Send,
        S2: Service<Parts, Response=GuardServiceResponse,Error=(StatusCode,String)> + Send + Clone + 'static,
        <S2 as Service<Parts>>::Future: Send,{
    left:S1,
    right:S2,
}
impl<S1,S2> OrGuardService<S1,S2>
    where
        S1: Service<Parts, Response=GuardServiceResponse,Error=(StatusCode,String)> + Send + Clone + 'static,
        <S1 as Service<Parts>>::Future: Send,
        S2: Service<Parts, Response=GuardServiceResponse,Error=(StatusCode,String)> + Send + Clone + 'static,
        <S2 as Service<Parts>>::Future: Send, {
    pub fn new(left:S1,right:S2) -> Self{
        Self{ left, right }
    }
}
impl<S1,S2> Service<Parts> for OrGuardService<S1,S2>
    where
        S1: Service<Parts, Response=GuardServiceResponse,Error=(StatusCode,String)> + Send + Clone + 'static,
        <S1 as Service<Parts>>::Future: Send,
        S2: Service<Parts, Response=GuardServiceResponse,Error=(StatusCode,String)> + Send + Clone + 'static,
        <S2 as Service<Parts>>::Future: Send, {
    type Response = GuardServiceResponse;
    type Error = (StatusCode,String);
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
            if result.0{
                Ok(GuardServiceResponse((true,None), parts))
            } else {
                right.call(parts).await
            }
        })

    }
}


pub struct GuardServiceResponse( (bool,Option<String>), Parts);



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
        type Rejection = (StatusCode,String);

        async fn from_request_parts(parts: &mut Parts, state: &ArbitraryData) -> Result<Self, Self::Rejection> {
            Ok(Self {
                data: parts.headers.get(state.data.clone())
                    .ok_or((StatusCode::INTERNAL_SERVER_ERROR,"error".into()))?
                    .to_str()
                    .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR,"error".into()))?
                    .to_string()
            })
        }
    }
    async fn ok() -> StatusCode {
        StatusCode::OK
    }
    use super::*;

    #[tokio::test]
    async fn test_guard_service_ok() {
        let (mut parts, _) = Request::new(()).into_parts();
        parts.headers.insert(
            "data",
            HeaderValue::from_static("other_data"));
        assert_eq!(GuardService::new(
            ArbitraryData { data: "data".into() },
            ArbitraryData { data: "other_data".into() },"err")
            .call(parts).await.unwrap().0,(true,None));
    }

    #[tokio::test]
    async fn test_guard_service_not_from_request_parts_error() {
        let (mut parts, _) = Request::new(()).into_parts();
        parts.headers.insert(
            "BIG-UH-OH",
            HeaderValue::from_static("other_data"));
        let result = GuardService::new(
            ArbitraryData { data: "data".into() },
            ArbitraryData { data: "other_data".into() },"err")
            .call(parts).await;
        assert_eq!(result.err().map(|err|err.0), Some(StatusCode::INTERNAL_SERVER_ERROR));
    }

    #[tokio::test]
    async fn test_guard_service_expected_failed() {
        let (mut parts, _) = Request::new(()).into_parts();
        parts.headers.insert(
            "data",
            HeaderValue::from_static("other_data"));
        assert_eq!(GuardService::new(
            ArbitraryData { data: "data".into() },
            ArbitraryData { data: "NOT OTHER DATA MY BAD".into() },"err")
            .call(parts).await.unwrap().0,(false,Some("err".into())));
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
                    data.clone(), data.clone(),"err"
                ),
                GuardService::new(
                    other_data.clone(), other_data.clone(),"err"
                )
            ).call(parts).await.unwrap().0.0
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
                    data.clone(), data.clone(),"err"
                ),
                GuardService::new(
                    other_data.clone(), other_data.clone(),"err"
                )
            ).call(parts).await.unwrap().0.0
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
                            data.clone(), data.clone(),"err"
                        ),
                        GuardService::new(
                            data.clone(), data.clone(),"err"
                        ),
                    ),
                    AndGuardService::new(
                        GuardService::new(
                            data.clone(), data.clone(),"err"
                        ),
                        GuardService::new(
                            data.clone(), data.clone(),"err"
                        )
                    )
                ),
                AndGuardService::new(
                    AndGuardService::new(
                        GuardService::new(
                            data.clone(), data.clone(),"err"
                        ),
                        GuardService::new(
                            data.clone(), data.clone(),"err"
                        ),
                    ),
                    AndGuardService::new(
                        GuardService::new(
                            data.clone(), data.clone(),"err"
                        ),
                        GuardService::new(
                            data.clone(), data.clone(),"err"
                        )
                    )
                )
            ).call(parts).await.unwrap().0.0
        )
    }
    #[tokio::test]
    async fn test_layer() {
        let req = Request::builder()
            .header("data","data")
            .body(BoxBody::default())
            .unwrap();

        let data = ArbitraryData { data: "data".into() };
        let app = Router::new()
            .route("/", get(ok))
            .layer(
                GuardLayer::with(
                    GuardService::new(
                        data.clone(),
                        data.clone(),"err"))
            );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(),StatusCode::OK)
    }
    #[tokio::test]
    async fn test_layer_or_ok() {
        let req = Request::builder()
            .header("data","data")
            .header("not_data","wazzup i'm a criminal")
            .body(BoxBody::default())
            .unwrap();

        let data = ArbitraryData { data: "data".into() };
        let app = Router::new()
            .route("/", get(ok))
            .layer(
                GuardLayer::with(
                    GuardService::new(
                        data.clone(),
                        data.clone(),"err")
                        .or(GuardService::new(
                            ArbitraryData{data:"not_data".into()},
                            data,"err"
                        ))
                )
            );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(),StatusCode::OK)
    }
    #[tokio::test]
    async fn test_layer_or_not_ok() {
        let req = Request::builder()
            .header("data","wazzup i'm a criminal")
            .header("not_data","wazzup i'm a criminal")
            .body(BoxBody::default())
            .unwrap();

        let data = ArbitraryData { data: "data".into() };
        let app = Router::new()
            .route("/", get(ok))
            .layer(
                GuardLayer::with(
                    GuardService::new(
                        data.clone(),
                        data.clone(),"err")
                        .or(GuardService::new(
                            ArbitraryData{data:"not_data".into()},
                            data,"err"
                        ))
                )
            );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(),StatusCode::UNAUTHORIZED)
    }
    #[tokio::test]
    async fn test_layer_and_ok() {
        let req = Request::builder()
            .header("data","data")
            .header("not_data","not_data")
            .body(BoxBody::default())
            .unwrap();

        let data = ArbitraryData { data: "data".into() };
        let definitely_still_data = ArbitraryData{data:"not_data".into()};
        let app = Router::new()
            .route("/", get(ok))
            .layer(
                GuardLayer::with(
                    GuardService::new(
                        data.clone(),
                        data.clone(),"err")
                        .and(GuardService::new(
                            definitely_still_data.clone(),
                            definitely_still_data,"err"
                        ))
                )
            );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(),StatusCode::OK)
    }
    #[tokio::test]
    async fn test_layer_and_not_ok() {
        let req = Request::builder()
            .header("data","data")
            .header("not_data","GRRR I SO HACK U")
            .body(BoxBody::default())
            .unwrap();

        let data = ArbitraryData { data: "data".into() };
        let definitely_still_data = ArbitraryData{data:"not_data".into()};
        let app = Router::new()
            .route("/", get(ok))
            .layer(
                GuardLayer::with(
                    GuardService::new(
                        data.clone(),
                        data.clone(),"err")
                        .and(GuardService::new(
                            definitely_still_data.clone(),
                            definitely_still_data,"err"
                        ))
                )
            );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(),StatusCode::UNAUTHORIZED)
    }

    #[tokio::test]
    async fn test_deep_layer_ok() {
        let req = Request::builder()
            .header("1","1")
            .header("2","2")
            .header("3","3")
            .header("4","4")
            .header("5","5")
            .header("6","6")
            .header("7","8")
            .body(BoxBody::default())
            .unwrap();

        let one = ArbitraryData { data: "1".into() };
        let two = ArbitraryData { data: "2".into() };
        let three = ArbitraryData { data: "3".into() };
        let four = ArbitraryData { data: "4".into() };
        let five = ArbitraryData { data: "5".into() };
        let six = ArbitraryData { data: "6".into() };
        let seven = ArbitraryData { data: "7".into() };
        let bad = ArbitraryData { data: "bad".into() };

        let app = Router::new()
            .route("/", get(ok))
            .layer(
                GuardLayer::with(
                    GuardService::new(
                        one.clone(),
                        one.clone(),"err").and(
                        GuardService::new(
                            two.clone(),
                            bad.clone(),"err"
                        ).or(
                            GuardService::new(
                                three.clone(),
                                bad.clone(),"err"
                            )
                                .or(
                                    GuardService::new(
                                        four.clone(),
                                        four.clone(),"err"
                                    )
                                )
                        )
                    ).and(
                        GuardService::new(five.clone(), five.clone(),"err"
                        )
                    ).and(
                        GuardService::new(six.clone(),six.clone(),"err")
                    )
                        .or(GuardService::new(seven.clone(),bad.clone(),"err")))
            );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(),StatusCode::OK)
    }
    async fn time_time<B>(req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
        sleep(Duration::from_millis(10)).await;
        Ok(next.run(req).await)
    }
    #[tokio::test]
    async fn test_into_layer() {
        let req = Request::builder()
            .header("data","data")
            .body(BoxBody::default())
            .unwrap();

        let data = ArbitraryData { data: "data".into() };
        let app = Router::new()
            .route("/", get(ok))
            .layer(
                GuardService::new(data.clone(), data.clone(),"err")
                    .into_layer()
            );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(),StatusCode::OK)
    }
    #[tokio::test]
    async fn test_happy_with_layered_polls() {
        let app = Router::new()
            .route("/",get(ok))
            .layer(axum::middleware::from_fn(time_time))
            .layer(GuardLayer::with(
                GuardService::new(
                    ArbitraryData{data:"x".into()},ArbitraryData{data:"x".into()},"err"
                )))
            .layer(axum::middleware::from_fn(time_time))
            .layer(tower_http::timeout::TimeoutLayer::new(Duration::from_secs(1)));
        let response = app
            .oneshot(
                Request::builder()
                    .header("x","x")
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

}
