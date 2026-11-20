# Microsoft WebView2 loader

The Windows build of `webview2-com-sys` 0.38.2 statically links
`WebView2LoaderStatic.lib` from Microsoft's WebView2 SDK 1.0.3650.58.
The crate's x64 library matches `build/native/x64/WebView2LoaderStatic.lib`
in the [pinned NuGet package](https://www.nuget.org/packages/Microsoft.Web.WebView2/1.0.3650.58).

[`LICENSE.txt`](LICENSE.txt) and [`NOTICE.txt`](NOTICE.txt) were extracted
unchanged from the [NuGet archive](https://api.nuget.org/v3-flatcontainer/microsoft.web.webview2/1.0.3650.58/microsoft.web.webview2.1.0.3650.58.nupkg).
The download matched the SHA-512 in [NuGet's catalog record](https://api.nuget.org/v3/catalog0/data/2025.12.08.14.05.37/microsoft.web.webview2.1.0.3650.58.json).
The catalog publishes this SHA-512 in Base64.

```text
WDaTuak798It8pY8Y+2DG68qWZKwo5u1Ux8kDvaDiUexmsDhfqWdw1LR87YkEqdPa/14SEywVZRWzgtNoKKqqw==
```

The SHA-256 digests below were calculated from the verified archive and its
contents.

| File | SHA-256 |
| --- | --- |
| `microsoft.web.webview2.1.0.3650.58.nupkg` | `911a472128c82ac8baa0c486c23342cc9dd6e7dc50d754e676726642ca065c60` |
| `LICENSE.txt` | `0af8f1b807512aae39c2ac1aa4d0cae65cabecb6fd554b8439a5162a0d6eca55` |
| `NOTICE.txt` | `106423785c5b7eba0a8e61d1837f2132e9c828e20ad530f565d981c1df60dd90` |
| `build/native/x64/WebView2LoaderStatic.lib` | `0659b741bde6348d4c4a6ec4ceb9af50e3d0048ed9cd3c8659bccbb61fde55ee` |
