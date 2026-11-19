# markdown 1.0.0: contained parser panics

Copied from the crates.io `markdown-1.0.0` package on 15 November 2026:
`Cargo.toml`, `src/`, `license` (MIT), and `README.md`. All upstream bytes are
unchanged except the single public `to_mdast` function below.

List/fence combinations can panic in `to_mdast::compile`, including with GPUI
Kit 0.9.1's GFM options and `math_text` / `math_flow` enabled:

~~~text
1. ```
- x
~~~

~~~text
1. a
   ```
- x
~~~

The second input still failed after IRC's real markup rewrite appended a closing
fence. Kit calls this function both synchronously and on its background parser,
so containing panics only in a render preflight did not protect every path.
The patch returns the normal error type and a fixed reason instead. It requires
unwinding; Subspace's dev and release profiles explicitly use `panic = "unwind"`,
and packaging uses release. Upstream declares `no_std`, so the one-function patch
imports `std` locally. Subspace is a std application.

Remove this patch and all three workspace manifest overrides once an upstream
release fixes these parser panics. Do not extend this fork with unrelated changes.

Exact source diff:

```diff
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -158,7 +158,18 @@
 /// # }
 /// ```
 pub fn to_mdast(value: &str, options: &ParseOptions) -> Result<mdast::Node, message::Message> {
-    let (events, parse_state) = parser::parse(value, options)?;
-    let node = to_mdast::compile(&events, parse_state.bytes)?;
-    Ok(node)
+    extern crate std;
+    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
+        let (events, parse_state) = parser::parse(value, options)?;
+        let node = to_mdast::compile(&events, parse_state.bytes)?;
+        Ok(node)
+    }))
+    .unwrap_or_else(|_| {
+        Err(message::Message {
+            place: None,
+            reason: String::from("The Markdown parser failed on this text."),
+            rule_id: alloc::boxed::Box::new(String::from("parser-panic")),
+            source: alloc::boxed::Box::new(String::from("markdown-rs")),
+        })
+    })
 }
```
