# axum_guard_combinator
https://crates.io/crates/axum_guard_combinator

Combine types that implement FromRequestParts and Guard with logical combinator inside Router layers.
Supply expected input inside of your layers and check the values extracted from requests against expected(supplied) values
inside you Guard impl.

```rust
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

```

You can nest your combinators to arbitrary depths
```rust
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
```
You can input data in the layer which will be evaluated in the Guard impl's check_guard method
```rust
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
```
