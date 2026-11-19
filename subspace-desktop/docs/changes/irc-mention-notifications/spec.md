# Spec: IRC mention notifications

## When a line notifies (`spaces/irc/src/state`)

A runtime event raises an `Attention` only when every one of these holds:

1. It is a live `Event::Line`. `Event::History` never raises one, and nor
   does `Event::Replayed`. The client sends that for lines in ZNC's
   `znc.in/playback` batch, which replays a bouncer's buffer on connect
   outside `chathistory`. A replayed line is appended and counted like a
   live one, so the badges show it.
2. The buffer does not already hold a line with the same `msgid`.
3. The line counts as unread (`counts::count`):
   - It is not the person's own line, including `echo-message` echoes.
   - It is a message: `PRIVMSG`, `ACTION` or `NOTICE`.
   - It is not in the **Server** buffer.
   - When the buffer has a read marker, the line's server time is after it.
4. It mentions the person (`Entry::highlight`: the runtime's current-nick
   match, such as `@nick`, or a highlight word), or it is in a private
   conversation.

`Attention` carries the buffer, the kind (`Highlight` or `Query`) and the
sender's nick.

The hub then drops attention for every conversation in view, while the board
and its window are both active:

- `Hub::visible`: the focused conversation, which also reads as lines arrive.
- `Hub::shown`: the displayed tab of every tab group (`Layout::shown`), so a
  conversation beside or below the focused one in a split stays quiet. A tab
  behind another in its group is not shown.

The board publishes both on focus changes, dock layout changes and window
activation. When the window is inactive, nothing is in view.

## What a notification says (`spaces/irc/src/notice.rs`)

| Buffer | Text (toast and OS title) | OS body |
| --- | --- | --- |
| Channel, sender known | `alice mentioned you in #subspace` | Network name |
| Channel, no sender | `You were mentioned in #subspace` | Network name |
| Private conversation | `carol sent you a private message` | Network name |

The text never includes the message.

## Board notices (`subspace_dashboard::notifications`)

- `Notice` has private `text` and `system_body`, read through accessors.
  `Notice::new(text)` leaves `system_body` empty; `Notice::described(text,
  body)` sets it.
- `request(source, target, notice, cx)` and `Sink` carry a `Notice`.
- The shell's OS post (`framework/host/app/src/notifications/runtime.rs`)
  shows `text` as the title and `system_body` as the body when `system_body`
  is set. Otherwise it shows the product name and "New activity is
  available." In-app toasts show `text`, as before.

## Board defaults

- `BoardSettings::notification_defaults: Preferences`, `Off` by default.
- The shell installs every notifying board's defaults (`notifications::Defaults`)
  with its registrations.
- `Preferences::save` stores the mode, style and sound with the marker
  `notifications.<source>.chosen`.
- `notifications::preferences(source, cx)` returns the stored values when
  the marker is set, or when they differ from Off, Standard and Silent.
  Otherwise it returns the board's defaults. Settings in 0.2.1 and earlier
  stored every board's Off, Standard and Silent default on any Save, so that
  unmarked value is no choice. A person who chose exactly that before gets
  the new default.
- Settings shows the same values and saves a source only when its choices
  differ from what it loaded, so an unrelated Save stores no choice.
- IRC declares `Mode::System`, `Style::Standard` and `Sound::Glass`. Where
  Glass is unavailable, as on Linux, the sound reads as Silent.

## Choosing a notification

- An OS response routes to `Route::Board("irc", target)`. The shell asks the
  platform to activate the app (`App::activate(true)`) and then raises the
  product window, or opens one. The board resolves the target to its buffer,
  shows it as the focused tab and selects its row. Showing it reads it.
- An in-app toast click follows the same route without the activation call.
