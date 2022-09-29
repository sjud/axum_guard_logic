# axum_guard_logic
https://crates.io/crates/axum_guard_logic

Impl `Guard` for `T`
```rust
pub trait Guard {
    fn check_guard(&self, expected:&Self) -> bool;
}
```
Where the values for self are extracted from the request based on `FromRequestParts<State> for T`
and the values for expected are given inside the layer on the route.

```rust
Router::with(state.clone())
            .route("/", get(ok))
            .layer(
                    GuardService::new(
                        state.clone(),
                        expected.clone()).into_layer()
            );
 ```
The argument state passed to `GuardService::new()` will be the state called
inside the `FromRequestParts` implementation on `T`

You can also nest using AND/OR logic.

```rust
Router::new()
    .route("/", get(ok))
    .layer(
        GuardService::new(state.clone(), expected)
            .and(
                GuardService::new(
                    other_state.clone(), 
                    other_expected
                )
            )
        .into_layer()
);
 ```
 Will reject `StatusCode::UNAUTHORIZED` when `check_guard` returns false.
