# Space Lab help screenshots

These WebP images are compiled into the app and registered by
`src/help_images.rs`. Native Help and the `board_help` image action use the
same bytes. The main guide references stable image IDs, not machine file paths.

- `dashboard.webp`: local starter controls, chart/table and result metadata.
- `apply.webp`: saved working source differs from the still-running snapshot.
- `settings.webp`: private variable names and explicit selected edit choices.

Capture only the isolated documentation project with synthetic data and
non-working settings. Do not include user projects, actual credentials, API
responses, editor buffers or unrelated windows. Review each rendered image
before staging it. Temporary full captures belong in `.local/docs/agents/`.

Encode reviewed native captures with `cwebp -q 86 input.png -o output.webp`.
Keep every image below the catalog's 512 KiB limit. After replacing images,
rebuild the product and verify both native Help rendering and tool readback.
