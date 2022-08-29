use std::task::{Poll,Context};
use std::mem::replace;
use std::fmt::Debug;
use std::marker::PhantomData;
use axum_core::extract::{FromRef, FromRequestParts};
use axum_core::response::{IntoResponse, Response};
use tower_layer::Layer;
use tower_service::Service;
use http::{Request, StatusCode};
use async_trait::async_trait;
use http::request::Parts;
use axum::RequestPartsExt;
use futures_core::future::BoxFuture;


pub trait Guard<State> {
    fn check_guard(&self, expected:&Self) -> bool;
}
impl<State,T: Guard<State> + Clone + FromRequestParts<State>> GuardExt<State> for T {}

pub trait GuardExt<State>: Guard<State> + Sized + Clone + FromRequestParts<State> {
    fn and<Right: 'static + Guard<State> + Clone + FromRequestParts<State>>(self, other: Right)
        -> And<Self, Right,State> {
        And(self, other,PhantomData)
    }

    fn and_with_sub_state<
        SubState:FromRef<State>,
        Right:'static + Guard<SubState> + Clone + FromRequestParts<SubState>>
    (self, other:Right) -> AndWithSubState<Self,Right,State,SubState>{
        AndWithSubState(self,other,PhantomData)
    }

    fn or<Right: 'static + Guard<State> + Clone + FromRequestParts<State>>(self, other: Right)
        -> Or<Self, Right,State> {
        Or(self, other,PhantomData)
    }

    fn or_with_sub_state<
        SubState:FromRef<State>,
        Right:'static + Guard<SubState> + Clone + FromRequestParts<SubState>>
    (self, other:Right) -> OrWithSubState<Self,Right,State,SubState>{
        OrWithSubState(self,other,PhantomData)
    }
}
pub struct OrWithSubState<Left,Right,State,SubState>(Left,Right,PhantomData<(State,SubState)>)
    where
        Left:'static + Guard<State> + Clone + FromRequestParts<State>,
        Right: 'static + Guard<SubState> + Clone + FromRequestParts<SubState>;

impl<Left,Right,State,SubState> Clone for OrWithSubState<Left,Right,State,SubState>
    where
        Left:'static + Guard<State> + Clone + FromRequestParts<State>,
        Right: 'static + Guard<SubState> + Clone + FromRequestParts<SubState>{
    fn clone(&self) -> Self {
        Self(self.0.clone(),self.1.clone(),PhantomData)
    }
}
#[async_trait::async_trait]
impl<Left,Right,State,SubState> FromRequestParts<State> for OrWithSubState<Left,Right,State,SubState>
    where
        State: Send + Sync,
        SubState:Clone + FromRef<State> + Send + Sync,
        Left:'static + Guard<State> + Clone + FromRequestParts<State> + Send,
        Right: 'static + Guard<SubState> + Clone + FromRequestParts<SubState> {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &State) -> Result<Self, Self::Rejection> {
        let left = <Left as FromRequestParts<State>>::from_request_parts(parts,state)
            .await
            .map_err(|err|StatusCode::INTERNAL_SERVER_ERROR)?;
        let right = <Right as FromRequestParts<SubState>>::from_request_parts(
            parts,
            &SubState::from_ref(state)).await
            .map_err(|err|StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Self(left,right,PhantomData))
    }
}
impl<Left, Right, State, SubState> Guard<State> for OrWithSubState<Left, Right, State, SubState>
    where
        Left:'static + Guard<State> + Clone + FromRequestParts<State>,
        Right: 'static + Guard<SubState> + Clone + FromRequestParts<SubState> {
    fn check_guard(&self, expected: &Self) -> bool {
        self.0.check_guard(&expected.0) || self.1.check_guard(&expected.1)
    }
}
pub struct AndWithSubState<Left,Right,State,SubState>(Left,Right,PhantomData<(State,SubState)>)
    where
        Left:'static + Guard<State> + Clone + FromRequestParts<State>,
        Right: 'static + Guard<SubState> + Clone + FromRequestParts<SubState>;

impl<Left,Right,State,SubState> Clone for AndWithSubState<Left,Right,State,SubState>
    where
        Left:'static + Guard<State> + Clone + FromRequestParts<State>,
        Right: 'static + Guard<SubState> + Clone + FromRequestParts<SubState>{
    fn clone(&self) -> Self {
        Self(self.0.clone(),self.1.clone(),PhantomData)
    }
}
#[async_trait::async_trait]
impl<Left,Right,State,SubState> FromRequestParts<State> for AndWithSubState<Left,Right,State,SubState>
    where
        State: Send + Sync,
        SubState:Clone + FromRef<State> + Send + Sync,
        Left:'static + Guard<State> + Clone + FromRequestParts<State> + Send,
        Right: 'static + Guard<SubState> + Clone + FromRequestParts<SubState> {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &State) -> Result<Self, Self::Rejection> {
        let left = <Left as FromRequestParts<State>>::from_request_parts(parts,state)
            .await
            .map_err(|err|StatusCode::INTERNAL_SERVER_ERROR)?;
        let right = <Right as FromRequestParts<SubState>>::from_request_parts(
            parts,
            &SubState::from_ref(state)).await
            .map_err(|err|StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Self(left,right,PhantomData))
    }
}

impl<Left, Right, State, SubState> Guard<State> for AndWithSubState<Left, Right, State, SubState>
    where
        Left:'static + Guard<State> + Clone + FromRequestParts<State>,
        Right: 'static + Guard<SubState> + Clone + FromRequestParts<SubState> {
    fn check_guard(&self, expected: &Self) -> bool {
        self.0.check_guard(&expected.0) && self.1.check_guard(&expected.1)
    }
}


pub struct And<Left,Right,State>(Left, Right, PhantomData<State>)
    where
    Left:'static + Guard<State> + Clone + FromRequestParts<State>,
    Right: 'static + Guard<State> + Clone + FromRequestParts<State>;

impl<Left,Right,State> Clone for And<Left,Right,State>
    where
        Left:'static + Guard<State> + Clone + FromRequestParts<State>,
        Right: 'static + Guard<State> + Clone + FromRequestParts<State> {
    fn clone(&self) -> Self {
        Self(self.0.clone(),self.1.clone(),PhantomData)
    }
}

impl<Left,Right,State> Guard<State> for And<Left,Right,State>
    where
    Left:'static + Guard<State> + Clone + FromRequestParts<State>,
    Right: 'static + Guard<State> + Clone + FromRequestParts<State> {
    fn check_guard(&self, expected: &Self) -> bool {
        self.0.check_guard(&expected.0) && self.1.check_guard(&expected.1)
    }
}

#[async_trait::async_trait]
impl<Left,Right,State> FromRequestParts<State> for And<Left,Right,State>
    where
        State:Send+Sync,
        Left:'static + Guard<State> + Clone + FromRequestParts<State> + Send,
        Right: 'static + Guard<State> + Clone + FromRequestParts<State> + Send{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &State) -> Result<Self, Self::Rejection> {
        let left = Left::from_request_parts(parts,state).await
            .map_err(|err|StatusCode::INTERNAL_SERVER_ERROR)?;
        let right = Right::from_request_parts(parts,state).await
            .map_err(|err|StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Self(left,right,PhantomData))
    }
}


pub struct Or<Left,Right,State>(Left, Right,PhantomData<State>) where
    Left:'static + Guard<State> + Clone + FromRequestParts<State>,
    Right: 'static + Guard<State> + Clone + FromRequestParts<State>;

impl<Left,Right,State> Clone for Or<Left,Right,State>
    where
        Left:'static + Guard<State> + Clone + FromRequestParts<State>,
        Right: 'static + Guard<State> + Clone + FromRequestParts<State> {
    fn clone(&self) -> Self {
        Self(self.0.clone(),self.1.clone(),PhantomData)
    }
}

impl<Left, Right,State> Guard<State> for Or<Left, Right,State> where
    Left:'static + Guard<State> + Clone + FromRequestParts<State>,
    Right: 'static + Guard<State> + Clone + FromRequestParts<State> {
    fn check_guard(&self, expected: &Self) -> bool {
        self.0.check_guard(&expected.0) || self.1.check_guard(&expected.1)
    }
}

#[async_trait::async_trait]
impl<Left,Right,State> FromRequestParts<State> for Or<Left,Right,State>
    where
        State:Send+Sync,
        Left:'static + Guard<State> + Clone + FromRequestParts<State> + Send,
        Right: 'static + Guard<State> + Clone + FromRequestParts<State> + Send{
    type Rejection = StatusCode;


    async fn from_request_parts(parts: &mut Parts, state: &State) -> Result<Self, Self::Rejection> {
        let left = Left::from_request_parts(parts,state).await
            .map_err(|err|StatusCode::INTERNAL_SERVER_ERROR)?;
        let right = Right::from_request_parts(parts,state).await
            .map_err(|err|StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Self(left,right,PhantomData))
    }
}

pub struct GuardLayer<G,B,State> {
    expected_guard: Option<G>,
    state:State,
    _marker: PhantomData<B>,
}

impl<G,B,State> GuardLayer<G,B,State>{
    pub fn with(state:State, expected_guard:G) -> Self {
        Self{
            state,
            expected_guard:Some(expected_guard),
            _marker: PhantomData
        }
    }
}

impl<S,B,G,State> Layer<S> for GuardLayer<G,B,State>
    where
        State:Clone,
        S: Service<Request<B>> + Clone + Send,
        B: Send + Sync,
        G: Guard<State> + FromRequestParts<State> + Send + Sync + Clone{
    type Service = GuardService<S,G,B,State>;

    fn layer(&self, inner: S) -> Self::Service {
        GuardService{
            expected_guard: self.expected_guard.clone(),
            state:self.state.clone(),
            inner,
            _marker: PhantomData
        }
    }
}

pub struct GuardService<S,G,B,State>
    where
        S: Service<Request<B>> + Clone + Send,
        B: Send + Sync,
        G: Guard<State> + FromRequestParts<State> + Send + Sync + Clone {
    expected_guard:Option<G>,
    state:State,
    inner:S,
    _marker:PhantomData<B>,
}
impl<S,G,B,State> Clone for GuardService<S,G,B,State>
    where
        State:Clone,
        S: Service<Request<B>> + Clone + Send,
        B: Send + Sync,
        G: Guard<State> + FromRequestParts<State> + Send + Sync + Clone{
    fn clone(&self) -> Self {
        Self{
            expected_guard: self.expected_guard.clone(),
            inner: self.inner.clone(),
            state: self.state.clone(),
            _marker: PhantomData
        }
    }
}

impl<G,S,ResBody,B,State> Service<Request<B>> for GuardService<S,G,B,State>
    where
        State:Send+Sync+Clone+'static,
        ResBody:Default,
        S: Service<Request<B>, Response = Response<ResBody>> + Clone + Send + 'static,
        <S as Service<Request<B>>>::Future:Send,
        G: Guard<State> + FromRequestParts<State> + Sync + Send + Clone + 'static,
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
        let state = self.state.clone();
        let mut inner = replace(&mut self.inner, clone);
        Box::pin(
        async move {
            let (mut parts,body) = req.into_parts();
            let guard : G = match
                G::from_request_parts(&mut parts,&state).await {
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
    impl Guard<()> for ArbitraryData{
        fn check_guard(&self, expected: &Self) -> bool {
            *self == *expected
        }
    }
    #[async_trait::async_trait]
    impl FromRequestParts<()> for ArbitraryData {
        type Rejection = ();

        async fn from_request_parts(parts: &mut Parts, state: &()) -> Result<Self, Self::Rejection> {
            Ok(Self{
                data: parts.headers.get("data").unwrap()
                    .to_str().unwrap().to_string()
            })
        }
    }
    #[derive(Clone,Copy,Debug,PartialEq)]
    pub struct Always;

    impl Guard<()> for Always {
        fn check_guard(&self,_:&Self) -> bool {
            true
        }
    }
    #[async_trait::async_trait]
    impl FromRequestParts<()> for Always {
        type Rejection = ();

        async fn from_request_parts(parts: &mut Parts, state: &()) -> Result<Self, Self::Rejection> {
            Ok(Self)
        }
    }
    #[derive(Clone,Copy,Debug,PartialEq)]
    pub struct Never;
    impl Guard<()> for Never {
        fn check_guard(&self, expected: &Self) -> bool {
            false
        }
    }
    #[async_trait::async_trait]
    impl FromRequestParts<()> for Never {
        type Rejection = ();

        async fn from_request_parts(parts: &mut Parts, state: &()) -> Result<Self, Self::Rejection> {
            Ok(Self)
        }
    }
    #[derive(Copy,Clone,Debug)]
    pub struct StateGuardData(bool);

    impl Guard<State> for StateGuardData {
        fn check_guard(&self, expected: &Self) -> bool {
            self.0 == expected.0
        }
    }
    #[async_trait::async_trait]
    impl FromRequestParts<State> for StateGuardData {
        type Rejection = ();

        async fn from_request_parts(parts: &mut Parts, state: &State) -> Result<Self, Self::Rejection> {
            Ok(Self(state.0))
        }
    }

    #[derive(Copy,Clone,Debug)]
    pub struct State(bool);

    #[derive(Clone,Debug,PartialEq)]
    pub struct OtherStateGuardData(bool,String);

    impl Guard<(State,OtherState)> for OtherStateGuardData {
        fn check_guard(&self, expected: &Self) -> bool {
            self.0 == self.0 && self.1 == expected.1
        }
    }
    impl FromRef<(State,OtherState)> for State {
        fn from_ref(input: &(State, OtherState)) -> Self {
            input.0
        }
    }
    impl FromRef<(State,OtherState)> for OtherState{
        fn from_ref(input: &(State, OtherState)) -> Self {
            input.1.clone()
        }
    }
    #[async_trait::async_trait]
    impl FromRequestParts<(State,OtherState)> for OtherStateGuardData {
        type Rejection = StatusCode;

        async fn from_request_parts(parts: &mut Parts, state: &(State, OtherState))
                                    -> Result<Self, Self::Rejection> {
            Ok(Self(state.0.0,state.1.0.clone()))
        }
    }
    #[derive(Clone,Debug)]
    pub struct StringGuard(String);
    impl Guard<OtherState> for StringGuard {
        fn check_guard(&self, expected: &Self) -> bool {
            self.0 == expected.0
        }
    }
    #[async_trait::async_trait]
    impl FromRequestParts<OtherState> for StringGuard {
        type Rejection = ();

        async fn from_request_parts(parts: &mut Parts, state: &OtherState) -> Result<Self, Self::Rejection> {
            Ok(Self(state.0.clone()))
        }
    }
    #[derive(Clone,Debug)]
    pub struct OtherState(String);

    #[axum_macros::debug_handler]
    async fn ok() -> StatusCode { StatusCode::OK }

    #[tokio::test]
    async fn test_always() {
        let app = Router::new()
            .route("/",get(ok))
            .layer(GuardLayer::with((), Always, ));
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
            .layer(GuardLayer::with((), Never, ));
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
                .layer(GuardLayer::with((), Always.and(Always), )));
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
                .layer(GuardLayer::with((), Always.and(Never), )));
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
                .layer(GuardLayer::with((), Always.or(Never), )));
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
                .layer(GuardLayer::with((), Never.or(Never), )));
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
                .layer(GuardLayer::with((),
                                        Never.or(
                                    Always.and(
                                            Always.or(
                                                Never))),
                )));
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
                .layer(GuardLayer::with((), ArbitraryData{
                                    data:String::from("Hello World.")
                                },
                )));
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
            .layer(GuardLayer::with((), ArbitraryData{
                            data:String::from("Hello World.")
                        },
            ))
            .layer(
                GuardLayer::with((), ArbitraryData{
                                    data:String::from("Should fail.")},
                ));
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
            .layer(GuardLayer::with((), Always, ))
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


    #[tokio::test]
    async fn test_with_state_fail() {
        let state = State(false);
        let app = Router::with_state(state)
            .route("/",get(ok))
            .layer(GuardLayer::with(state, StateGuardData(true), ));
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
    async fn test_with_state_pass() {
        let state = State(true);
        let app = Router::with_state(state)
            .route("/",get(ok))
            .layer(GuardLayer::with(state, StateGuardData(true), ));
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
    async fn logic_layers_and_with_sub_states_happy() {
        let state = State(true);
        let other_state = OtherState("Hello world.".into());
        let super_state = (state,other_state);
        let app = Router::with_state(super_state.clone())
            .route("/",get(ok))
            .layer(GuardLayer::with(
                super_state.clone(),
                OtherStateGuardData(true,"Hello world.".into())
                .and_with_sub_state::<State, _>(StateGuardData(true))
                .and_with_sub_state::<OtherState,_>(StringGuard("Hello world.".into())
                ),
            ));
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
    async fn logic_layers_and_with_sub_states_sad() {
        let state = State(true);
        let other_state = OtherState("Hello world.".into());
        let super_state = (state,other_state);
        let app = Router::with_state(super_state.clone())
            .route("/",get(ok))
            .layer(GuardLayer::with(
                super_state.clone(),
                OtherStateGuardData(true,"Hello world.".into())
                    .and_with_sub_state::<State, _>(StateGuardData(true))
                    .and_with_sub_state::<OtherState,_>(StringGuard("GOODBYE CRUEL WORLD.".into())
                    ),
            ));
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
    async fn or_with_substate_happy() {
        let state = State(true);
        let other_state = OtherState("Hello world.".into());
        let super_state = (state,other_state);
        let app = Router::with_state(super_state.clone())
            .route("/",get(ok))
            .layer(GuardLayer::with(
                super_state.clone(),
                OtherStateGuardData(false,"BAD DATA.".into())
                    .or_with_sub_state::<State,_>(StateGuardData(true))
                     ));
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
    async fn or_with_substate_sad() {
        let state = State(true);
        let other_state = OtherState("Hello world.".into());
        let super_state = (state,other_state);
        let app = Router::with_state(super_state.clone())
            .route("/",get(ok))
            .layer(GuardLayer::with(
                super_state.clone(),
                OtherStateGuardData(false,"BAD DATA.".into())
                    .or_with_sub_state::<State,_>(StateGuardData(false))
            ));
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
    async fn deep_all_with_substate_happy() {
        let state = State(true);
        let other_state = OtherState("But actually yes.".into());
        let super_state = (state,other_state);
        let app = Router::with_state(super_state.clone())
            .route("/",get(ok))
            .layer(GuardLayer::with(
                super_state.clone(),
                OtherStateGuardData(true,"Hello world.".into())
                    .and_with_sub_state::<State,_>(StateGuardData(true))
                    .and_with_sub_state::<State,_>(StateGuardData(true))
                    .or_with_sub_state::<OtherState,_>(StringGuard("Nope.".into()))
                    .or_with_sub_state::<OtherState,_>(StringGuard("But actually yes.".into()))
            ));
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
    async fn deep_all_with_substate_sad() {
        let state = State(true);
        let other_state = OtherState("But actually yes.".into());
        let super_state = (state,other_state);
        let app = Router::with_state(super_state.clone())
            .route("/",get(ok))
            .layer(GuardLayer::with(
                super_state.clone(),
                OtherStateGuardData(true,"Hello world.".into())
                    .and_with_sub_state::<State,_>(StateGuardData(true))
                    .and_with_sub_state::<State,_>(StateGuardData(true))
                    .or_with_sub_state::<OtherState,_>(StringGuard("Nope.".into()))
                    .or_with_sub_state::<OtherState,_>(StringGuard("But actually yes.".into()))
                    .or(OtherStateGuardData(false,"Still yes, yay logic".into()))
                    .and_with_sub_state::<OtherState,_>(StringGuard("But not really.".into()))
            ));
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
    async fn more_deep_all_with_substate_happy() {
        let state = State(true);
        let other_state = OtherState("But actually yes.".into());
        let super_state = (state,other_state);
        let app = Router::with_state(super_state.clone())
            .route("/",get(ok))
            .layer(GuardLayer::with(
                super_state.clone(),
                OtherStateGuardData(true,"Hello world.".into())
                    .and_with_sub_state::<State,_>(StateGuardData(true))
                    .and_with_sub_state::<State,_>(StateGuardData(true))
                    .or_with_sub_state::<OtherState,_>(StringGuard("Nope.".into()))
                    .or_with_sub_state::<OtherState,_>(StringGuard("But actually yes.".into()))
                    .or(OtherStateGuardData(false,"Still yes, yay logic".into()))
                    .and(OtherStateGuardData(true,"But actually yes.".into()))
            ));
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

    /* DOESN'T COMPILE
    #[tokio::test]
    async fn layered_handler() {
        let layered = ok.layer(GuardLayer::with((),Always));
        let app = Router::new()
            .route("/",get(layered));
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
    */
}
