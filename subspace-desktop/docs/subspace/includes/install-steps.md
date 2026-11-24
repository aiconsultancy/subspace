<!-- markdownlint-disable MD041 -->

1. Select the download link for your platform above. You can also open the
   versioned release page to browse its assets.
2. Install the downloaded archive:
   - On Apple silicon macOS, unzip the app ZIP. Copy the `.app` bundle to
     `~/Applications` or `/Applications`, then open it from Finder. macOS 15
     or later is required.
   - On Windows x64, extract the ZIP to a folder you will keep, then open
     `subspace.exe`.
   - On Linux x64, extract the `.tar.gz` archive, then run `subspace` from the
     extracted folder.

Each release has two macOS apps. `Subspace_<version>_darwin_arm64.zip` signs
in to an API on this computer (`http://127.0.0.1:3000`), such as the local
stack or a fleet's tunnel; a [connection file](../connection-file.md) points
it at a hosted API instead. `Subspace-NoAPI_<version>_darwin_arm64.zip` has no
API and nothing to sign in to. Each updates only to its own kind.

Sign in to a GitHub account with access to the `aiconsultancy/subspace`
repository for releases and updates.
The full installation guide covers [update authentication](../../installation.md#token).
