# AgentsView guide

## Overview

AgentsView opens your fleet's session browser as a Space. Open the Space and
use the Subspace Help button to return to this guide.

## Before you start

AgentsView is included in standard macOS and Windows builds. You need the
fleet's AgentsView server address and, if the server requires it, an API
key from its administrator. An SSH tunnel to the fleet's local server also
works. Use a distinct local port for each authenticated fleet's SSH tunnel.
AgentsView sessions can contain private code and conversation history.

## Main view

The Space shows the fleet's own AgentsView interface. Its navigation and
session list come from that server. The Subspace status bar shows whether
the connection is ready.

## Toolbar and filters

Search and filter sessions using the embedded interface. Use Subspace's
Reload command to reconnect to the configured server.

## Details

Select a session in AgentsView to read its conversation and activity.

## Actions

Choose **Configure** in an unconfigured Space to open its connection settings.
If the server rejects the key, choose **Configure** and save a replacement.
The Space reconnects automatically. Use **Retry** after fixing a network issue.

## Keyboard shortcuts

The Space adds no Subspace shortcuts. Use the app's usual Space switcher
and command palette.

## Settings

Open **Settings › Spaces › AgentsView**. Enter the server address and paste
the API key, then choose **Save**. The saved key is masked during entry and
never appears in the embedded page. Use the clear control to remove or
replace it. To change servers, clear or replace the previous server's key.

## Troubleshooting

Check the address and that the fleet is reachable. For a local address, start
the SSH tunnel before opening AgentsView. If the fleet rejects the connection,
set a valid API key in Settings. Saving a changed key reconnects the Space.

## Limits

The Space connects to one AgentsView server at a time. The embedded
app routes inside its initial page. Use Subspace Reload for a fresh page.
Browser downloads and new windows are blocked.
Fleet data and session actions are provided by AgentsView itself.
