#![feature(pin_macro)]
#![feature(async_closure)]
use std::fmt::Debug;
use std::future::Future;
use std::marker::PhantomData;
use std::mem;
use std::pin::{Pin, pin};
use std::process::Output;
use std::task::{Context, Poll};
use axum_core::extract::{FromRequest, RequestParts};
use axum_core::response::{IntoResponse, Response};
use tower_layer::Layer;
use tower_service::Service;
use http::{Request, StatusCode};
use async_trait::async_trait;
/*
    .layer(GuardLayer::with(A{a_data}.and(B{b_data}.or(C{c_data}))))
    = And(A,Or(B,C))
*/


/// Implement Guard for a `Type{data:Data}` that implements FromRequest.
/// Then create a `GuardLayer::with(Type{data:Data{...})`
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

pub trait GuardExt<B>: Guard + Sized + Clone + FromRequest<B> {
    fn and<Right: Guard + Clone + FromRequest<B>>(self, other: Right) -> And<Self, Right,B> {
        And(self, other,PhantomData)
    }

    fn or<Right: Guard + Clone + FromRequest<B>>(self, other: Right) -> Or<Self, Right,B> {
        Or(self, other,PhantomData)
    }
}
impl<B,T: Guard + Clone + FromRequest<B>> GuardExt<B> for T {}

pub struct And<Left,Right,B>(Left, Right,PhantomData<B>) where
    Left: Guard + Clone + FromRequest<B>,
    Right: Guard + Clone + FromRequest<B>;

impl<Left,Right,B> Clone for And<Left,Right,B>
    where
        Left: Guard + Clone + FromRequest<B>,
        Right: Guard + Clone + FromRequest<B> {
    fn clone(&self) -> Self {
        Self(self.0.clone(),self.1.clone(),PhantomData)
    }
}

impl<Left,Right,B> Guard for And<Left,Right,B>where
    Left: Guard + Clone + FromRequest<B>,
    Right: Guard + Clone + FromRequest<B> {
    fn check_guard(&self, expected: &Self) -> bool {
        self.0.check_guard(&expected.0) && self.1.check_guard(&expected.1)
    }
}

#[async_trait::async_trait]
impl<Left,Right,B> FromRequest<B> for And<Left,Right,B>
    where
        B:Send,
        Left: Guard + Clone + FromRequest<B> + Send,
        Right: Guard + Clone + FromRequest<B> + Send{
    type Rejection = StatusCode;

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        let left = req.extract::<Left>().await
            .map_err(|err|StatusCode::INTERNAL_SERVER_ERROR)?;
        let right = req.extract::<Right>().await
            .map_err(|err|StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Self(left,right,PhantomData))
    }
}


pub struct Or<Left,Right,B>(Left, Right,PhantomData<B>) where
    Left: Guard + Clone + FromRequest<B>,
    Right: Guard + Clone + FromRequest<B>;

impl<Left,Right,B> Clone for Or<Left,Right,B>
    where
        Left: Guard + Clone + FromRequest<B>,
        Right: Guard + Clone + FromRequest<B> {
    fn clone(&self) -> Self {
        Self(self.0.clone(),self.1.clone(),PhantomData)
    }
}

impl<Left, Right, B> Guard for Or<Left, Right,B> where
    Left: Guard + Clone + FromRequest<B>,
    Right: Guard + Clone + FromRequest<B> {
    fn check_guard(&self, expected: &Self) -> bool {
        self.0.check_guard(&expected.0) || self.1.check_guard(&expected.1)
    }
}

#[async_trait::async_trait]
impl<Left,Right,B> FromRequest<B> for Or<Left,Right,B>
    where
        B:Send,
        Left: Guard + Clone + FromRequest<B> + Send,
        Right: Guard + Clone + FromRequest<B> + Send{
    type Rejection = StatusCode;

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        let left = req.extract::<Left>().await
            .map_err(|err|StatusCode::INTERNAL_SERVER_ERROR)?;
        let right = req.extract::<Right>().await
            .map_err(|err|StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Self(left,right,PhantomData))
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
        S: Service<Request<B>> + Clone,
        B: Send + Sync,
        G: Guard + FromRequest<B> + Send + Sync + Clone{
    type Service = GuardService<S,B,G>;

    fn layer(&self, inner: S) -> Self::Service {
        GuardService{
            expected_guard: self.expected_guard.clone(),
            inner,
            _marker: PhantomData
        }
    }
}

pub struct GuardService<S,B,G>
    where
        S: Service<Request<B>> + Clone,
        B: Send + Sync,
        G: Guard + FromRequest<B> + Send + Sync + Clone {
    expected_guard:Option<G>,
    inner:S,
    _marker:PhantomData<B>,
}
impl<S,B,G> Clone for GuardService<S,B,G>
    where
        S: Service<Request<B>> + Clone,
        B: Send + Sync,
        G: Guard + FromRequest<B> + Send + Sync + Clone{
    fn clone(&self) -> Self {
        Self{
            expected_guard: self.expected_guard.clone(),
            inner: self.inner.clone(),
            _marker: PhantomData
        }
    }
}

impl<G,B,S,ResBody> Service<Request<B>> for GuardService<S,B,G>
    where
        ResBody:Default,
        S: Service<Request<B>, Response = Response<ResBody>> + Clone,
        G: Guard + FromRequest<B> + Sync + Send + Clone,
        B: Send + Sync, {
    type Response = S::Response;
    type Error = S::Error;
    type Future = GuardFuture<G,S,B,ResBody>;

    fn poll_ready(&mut self, ctx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(ctx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let mut parts = Some(RequestParts::new(req));
        let clone = self.inner.clone();
        let inner = mem::replace(&mut self.inner, clone);
        GuardFuture{
            parts,
            // This is safe because GuardLayer can only be initialized from
            // It's with method, which requires a non option value.
            expected_guard:self.expected_guard.take().unwrap(),
            service:inner,
        }
    }
}

#[pin_project::pin_project]
pub struct GuardFuture<G,S,B,ResBody>
    where
        ResBody: Default,
        G: Guard + FromRequest<B>,
        S: Service<Request<B>, Response = Response<ResBody>>, {

    parts:Option<RequestParts<B>>,
    expected_guard:G,
    service: S,
}

impl<G,S,B,ResBody> Future for GuardFuture<G,S,B,ResBody> where
    ResBody: Default,
    G: Guard + FromRequest<B>,
    S: Service<Request<B>, Response = Response<ResBody>>, {
    type Output = Result<Response<ResBody>, S::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let expected = this.expected_guard;
        if let Some(parts) = this.parts {
            match pin!(parts.extract::<G>()).poll(cx) {
                Poll::Pending => {return Poll::Pending;},
                Poll::Ready(result) => {
                    match result {
                        Ok(guard) => {
                            if guard.check_guard(&expected) {}
                            else {
                                let mut res = Response::new(ResBody::default());
                                *res.status_mut() = StatusCode::UNAUTHORIZED;
                                return Poll::Ready(Ok(res));
                            }
                        },
                        Err(_) => {
                            let mut res = Response::new(ResBody::default());
                            *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                            return Poll::Ready(Ok(res));
                        }
                    }
                }
            };
            let req = this.parts.take().unwrap().try_into_request().unwrap();
            pin!(this.service.call(req)).poll(cx)
        } else {
            let mut res = Response::new(ResBody::default());
            *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            Poll::Ready(Ok(res))
        }
    }
}




#[cfg(test)]
pub mod tests {
    use std::time::Duration;
    use axum::body::Body;
    use axum::error_handling::{HandleError, HandleErrorLayer};
    use axum::handler::Handler;
    use axum::Router;
    use axum::routing::get;
    use http::StatusCode;
    use tower::util::ServiceExt;
    use axum::BoxError;
    use axum::middleware::Next;
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
    impl<B:Send+Sync> FromRequest<B> for ArbitraryData {
        type Rejection = ();

        async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
            Ok(Self{
                data: req.headers().get("data").unwrap()
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
    impl<B:Send+Sync> FromRequest<B> for Always {
        type Rejection = ();

        async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
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
    impl<B:Send+Sync> FromRequest<B> for Never {
        type Rejection = ();

        async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
            Ok(Self)
        }
    }
    async fn ok() -> StatusCode { StatusCode::OK }

    #[tokio::test]
    async fn test_always() {
        let app = Router::new()
            .route("/",get(ok
                .layer(GuardLayer::with(Always))));
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
            .route("/",get(ok
                .layer(GuardLayer::with(Never))));
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
            .route("/",get(ok
                .layer(GuardLayer::with(Always.and(Always)))));
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
            .route("/",get(ok
                .layer(GuardLayer::with(Always.and(Never)))));
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
            .route("/",get(ok
                .layer(GuardLayer::with(Always.or(Never)))));
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
            .route("/",get(ok
                .layer(GuardLayer::with(Never.or(Never)))));
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
            .route("/",get(ok
                .layer(GuardLayer::with(
                        Never.or(
                    Always.and(
                            Always.or(
                                Never)))))));
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
            .route("/",get(ok
                .layer(GuardLayer::with(ArbitraryData{
                    data:String::from("Hello World.")
                }))));
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
    async fn time_time<B>(req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
        std::thread::sleep(Duration::from_millis(500));
        Ok(next.run(req).await)
    }
    #[tokio::test]
    async fn test_happy_with_time_time() {
        let app = Router::new()
            .route("/",get(ok))
            .layer(axum::middleware::from_fn(time_time))
            .layer(GuardLayer::with(Always))
            .layer(axum::middleware::from_fn(time_time));
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