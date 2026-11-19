import { OpenApi } from "effect/http-api";
import { documentedApi } from "../src/api";

await Bun.write(
  new URL("../openapi.json", import.meta.url).pathname,
  `${JSON.stringify(OpenApi.fromApi(documentedApi), null, 2)}\n`,
);
