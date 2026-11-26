# Officer media in IRC

## Intent

Officers need to inspect issue and chat images and share documents or images in an IRC reply. Desktop already renders an attachment card when the message contains a valid upload reference.

## Specification

Return an MCP image block for a supported raster image after checking its size, dimensions and stored digest. Use the existing paged reader for text. Sharing an issue file creates a separate chat upload with the usual audience, expiry and revocation rules.

A process that can render or write files in an officer’s service can stage a new file for upload. A shell-free officer can share existing issue media or relay a reference supplied by the officer that rendered the file.

## Delivery and verification

- Register native image inspection and attachment sharing in the API.
- Install the staged-file tool and enable image input for the known vision models.
- Exercise the API against PostgreSQL, test the model request with a loopback provider, and verify the helper is included in the SSH bundle and an installed fleet.
- Roll out the API first. Inspect an existing image with a vision-capable officer and confirm that its IRC upload resolves to the same bytes.
