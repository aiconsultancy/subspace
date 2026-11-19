import { greeting } from "./app.js";

const output = document.getElementById("output");
greeting(globalThis.subspace)
  .then((text) => { output.textContent = text; })
  .catch((error) => { output.textContent = error.message; });
