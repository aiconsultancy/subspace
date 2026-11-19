# Weather web Space

A standalone Subspace web Space that shows the weather for a UK postcode. It has
a current-conditions header, a 24-hour forecast, a 7-day forecast and detail
tiles, over original full-window day and night scenes (clear, partly cloudy,
overcast, fog, rain, snow and storm).

It is plain HTML, CSS and JavaScript modules. It has no dependencies, no build
step and no native permissions. Subspace needs only these files. It is the
worked example in [Build and package a web Space](../../../docs/web-spaces-guide.md).

## Install

Use **Settings > Application > Web Spaces** in Subspace and add it from one of
these sources:

- **GitHub repository.** **Install Weather example…** fills this in: repository
  `aiconsultancy/subspace`, a blank **Ref** (the default branch; enter another
  branch, `tag:` or `commit:` if you need one) and the **Board directory**
  `subspace-desktop/web-spaces/examples/weather`. GitHub import is limited to
  organisations of the signed-in GitHub user.
- **Local folder.** Select `subspace-desktop/web-spaces/examples/weather/` from
  a checkout of this repository.
- **Local ZIP.** From `subspace-desktop/`, run
  `make package-web-space SPACE=web-spaces/examples/weather`. It validates the
  folder and writes `dist/web-spaces/weather-1.0.0.zip` without `test/`,
  `mise.toml` or other development files. Select that ZIP.

The manifest is `subspace-board.json` (the name is kept for compatibility). It
requests no permissions and declares no settings, so there is nothing to grant
after installing.

To run it in a normal browser instead, serve the folder with any static
file server and open `index.html`, for example
`python3 -m http.server --bind 127.0.0.1 8765`.

## Use

1. Enter a UK postcode, for example `SW1A 2AA`, and select **Show weather**.
2. Weather remembers the postcode in local storage on this device.
3. **Refresh** reloads the forecast. Requests are limited (see below).

Keyboard: Tab through the form, **Refresh** and the hourly controls. Focus the
hourly forecast and use Left, Right, Page Up, Page Down, Home and End to scroll
it. The scene animation stops when the system asks for reduced motion; all
weather information remains.

## Data and privacy

| Service | Used for | What it receives |
| --- | --- | --- |
| [Postcodes.io](https://postcodes.io/) | Postcode to coordinates and place name | The postcode you enter |
| [Open-Meteo](https://open-meteo.com/) | Current, hourly and daily forecast | The postcode's coordinates, rounded to 4 decimal places |

No other request is made. Nothing goes to GitHub, Subspace chat, analytics or
an image or font service. The page sends no cookies or referrer. Open-Meteo's
[privacy notice](https://open-meteo.com/en/terms#privacy) says it may keep
request logs, which can include coordinates, for 90 days.

Local storage holds the chosen location and the last forecast (about 30 KB). A
saved forecast older than 24 hours is discarded.

### Request limits and stale data

- A forecast is refreshed at most every 15 minutes while the page is visible.
  Refresh stops while the page is hidden and catches up when it is shown.
- A saved forecast less than 10 minutes old is used without a request.
- **Refresh** is limited to once a minute, or once every 15 seconds after a
  failure. Failures back off from one minute to 30 minutes. A rate-limit
  response pauses requests for at least five minutes, or for the
  `Retry-After` time the service gives.
- Each postcode lookup cancels the previous one, so a slow earlier answer can
  not replace a newer choice.
- If an update fails, Weather keeps the last forecast and labels it as saved,
  with its age and the reason. Data older than 30 minutes is always labelled
  as saved. It never shows old observations as current: humidity, wind and
  feels-like are hidden for saved data. If nothing is saved, it shows an error
  instead of a forecast.

## Preview scenes

`index.html?preview=NAME` shows invented sample data for checking the scenes
without a network request. NAME is one of `clear`, `partly`, `cloudy`, `fog`,
`drizzle`, `rain`, `heavyrain`, `snow` or `storm`, optionally followed by
`-night`, for example `?preview=storm-night`.

Preview data is not cached and cannot be mistaken for a live forecast: a yellow
banner, a "Synthetic preview" label and the title "Sample location" are always
shown, and the postcode form is disabled.

## Terms and attribution

- **Open-Meteo.** Weather data by [Open-Meteo.com](https://open-meteo.com/),
  licensed [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). The free
  hosted API is for **non-commercial** use only, with limits of 600 calls a
  minute, 5,000 an hour and 10,000 a day (see the
  [terms](https://open-meteo.com/en/terms)). This web Space uses it for evaluation
  and prototyping. It is not a commercial licence or a service guarantee. A
  commercial deployment needs an Open-Meteo paid plan or another provider; this
  web Space does not configure one, and it does not claim that any deployment is
  licensed.
- **Postcodes.io.** Source code is MIT licensed. Great Britain postcode data
  falls under the OS OpenData licence. **Northern Ireland postcode data (postcodes
  that start with "BT") may be used free for non-commercial use only. Commercial use
  needs a licence from NI Land & Property Services.** Weather shows a note for
  Northern Ireland results. See the
  [data licences](https://postcodes.io/docs/licences). Data includes Ordnance
  Survey, Royal Mail, National Statistics and NRS data, all Crown copyright and
  database right (or Royal Mail copyright and database right).
- Forecasts can be wrong. Do not use this web Space for safety decisions.

## Development

Run the offline checks with the pinned Node.js runtime through mise. From this
folder:

```sh
mise run test
```

From `subspace-desktop/`, `mise run weather-space-test` runs the same tests; it
is part of `make check`.

The tests use synthetic provider responses and make no network request. They
cover condition mapping, request URLs and error handling, caching, time zones
and daylight saving, refresh timing and the stale and error states, preview
labelling, text contrast in every scene, and package rules (no remote assets,
no inline scripts, no stray `fetch` or `innerHTML`).

Layout:

| Path | Purpose |
| --- | --- |
| `subspace-board.json`, `index.html`, `styles.css` | Manifest, page and styles |
| `src/main.js`, `src/ui.js`, `src/scene.js` | Page wiring, rendering, scene layers |
| `src/controller.js`, `src/refresh.js` | Location, forecast, cache and refresh timing |
| `src/api.js`, `src/postcode.js`, `src/forecast.js` | Providers, validation and parsing |
| `src/view-model.js`, `src/conditions.js`, `src/time.js` | Display data, WMO codes, time zones |
| `src/preview.js` | Synthetic preview data |
| `assets/icon.svg`, `src/icons.js` | Original local icons |

Scene colours are CSS variables in `styles.css`. `test/contrast.test.js` reads
them and checks the worst-case text contrast, so a palette change that hurts
readability fails the test.
