import { BunRuntime } from "@effect/platform-bun";
import { Layer } from "effect";
import { server } from "./api";
import { readDevKey } from "./auth";
import { config } from "./config";

const options = config();
const devKey = options.dev ? await readDevKey(options.dev.keyPath) : null;
BunRuntime.runMain(Layer.launch(server(options, devKey)));
