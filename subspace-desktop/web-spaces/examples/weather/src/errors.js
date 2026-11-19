/** Error kinds the app can recover from, each with a message safe to show. */
export const ERROR_MESSAGES = Object.freeze({
  "invalid-postcode":
    "That does not look like a UK postcode. Try a format such as SW1A 1AA.",
  "not-found":
    "No current UK postcode matches that entry. Check it and try again.",
  "no-coordinates":
    "That postcode has no map location, so a forecast is not available for it.",
  "rate-limit":
    "The weather service is limiting requests. Wait a few minutes and try again.",
  network:
    "Could not reach the weather service. Check your connection and try again.",
  timeout: "The weather service took too long to answer. Try again shortly.",
  server: "The weather service reported a problem. Try again later.",
  "bad-data": "The weather service sent data this app could not read.",
  aborted: "The request was cancelled.",
});

export class WeatherError extends Error {
  /**
   * @param {keyof typeof ERROR_MESSAGES} kind
   * @param {{ retryAfterMs?: number, status?: number }} [details]
   */
  constructor(kind, details = {}) {
    super(ERROR_MESSAGES[kind] ?? ERROR_MESSAGES.server);
    this.name = "WeatherError";
    this.kind = kind;
    this.retryAfterMs = details.retryAfterMs ?? null;
    this.status = details.status ?? null;
  }
}

/** Convert any thrown value into a WeatherError. */
export function toWeatherError(error) {
  return error instanceof WeatherError ? error : new WeatherError("network");
}
