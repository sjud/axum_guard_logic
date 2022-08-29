# axum_guard_logic
https://crates.io/crates/axum_guard_logic

Implement `Guard<State>` for a type `T` which implements `FromRequestParts<State>`.
```rust
#[derive(Clone,Debug,PartialEq)]
     pub struct ArbitraryData(bool);

 impl Guard<()> for ArbitraryData{
         fn check_guard(&self, expected: &Self) -> bool {
             self.0 == expected.0
         }
    }
#[async_trait::async_trait]
impl FromRequestParts<()> for ArbitraryData {
    type Rejection = StatusCode;

    async fn from_request_parts(_: &mut Parts, _: &()) -> Result<Self, Self::Rejection> {
        Ok(Self(true))
    }
}
````
Inside `impl<State> Guard<State> for T` you compare the value received from extracting `T`
on the request with
the expected value passed into the layer that you've layered onto your router.


For any type `SubState:FromRef<State>` 
and for all `G:Guard<SubState>+FromRequestParts<SubState>`
you can nest arbitrarily with the additional methods
`and_with_substate<SubState,_>(G)` and `or_with_substate<SubState,_>(G)`


```rust
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

  ```
BIG WARNING
Don
```rust
let layered = ok.layer(GuardLayer::with((),Always));
let app = Router::new()
.route("/",get(layered));
```
Currently this crate won't work with if layered directly on handlers, 
you'll receive a compiler error that looks like
```rust

error[E0277]: the trait bound `Layered<GuardLayer<Always, _, ()>, fn() -> impl Future<Output = http::StatusCode> {tests::ok}, ((),), _, _>: Handler<_, _, _>` is not satisfied
   --> src/lib.rs:876:28
    |
876 |             .route("/",get(layered));
    |                        --- ^^^^^^^ the trait `Handler<_, _, _>` is not implemented for `Layered<GuardLayer<Always, _, ()>, fn() -> impl Future<Output = http::StatusCode> {tests::ok}, ((),), _, _>`
    |                        |
    |                        required by a bound introduced by this call
    |
    = help: the trait `Handler<T, S, B>` is implemented for `Layered<L, H, T, S, B>`
note: required by a bound in `axum::routing::get`
   --> /Users/sam/.cargo/registry/src/github.com-1ecc6299db9ec823/axum-0.6.0-rc.1/src/routing/method_routing.rs:400:1
    |
400 | top_level_handler_fn!(get, GET);
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ required by this bound in `axum::routing::get`

```
You must layer on routes instead.

