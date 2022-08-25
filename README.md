# axum_guard_combinator
https://crates.io/crates/axum_guard_combinator

This library lets you write logical (OR, AND) combinators inside Tower Service Layers in Axum servers, 
which extract the given type T which implement Guard and checks the value of Type T
against the input value inside the layer's combinator.
Here's a struct which always evaluates to true.
```rust
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
```
Here's a test case combining said struct with a struct that always evaluates to false
```rust
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
