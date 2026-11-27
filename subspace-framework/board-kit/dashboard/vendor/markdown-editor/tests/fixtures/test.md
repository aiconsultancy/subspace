# Release notes

The **release** is ready for a _fixture review_. See the [checklist][review].

## Changes

- [x] Run the smoke test.
  - Capture the result.
- [ ] Share the first build.
  1. Verify the result.
  2. Write a short summary.

> Leave the rollout paused until feedback arrives.
>
> Keep the prior build available.

| Step | Result | Count |
| :--- | :---: | ---: |
| Build | **Passed** | 2 |
| Check | `ready` | 12 |

```rust
fn main() {
    println!("ready");
}
```

The message includes ![a fixture image](https://example.com/review.png) and
a <mark>highlight</mark>. Its label includes \[literal brackets\].

[review]: https://example.com/review "Review checklist"
