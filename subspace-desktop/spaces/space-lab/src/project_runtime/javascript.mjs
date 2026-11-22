/** Persistent Node/Bun helper. Export on_init/on_input/on_action(context,event).
 * Call context.publish(patch) from timers or streams for unsolicited updates.
 * Use ordinary package.json module selection, .mjs, or CommonJS exports.
 */
import { pathToFileURL } from 'node:url';

const MAX_FRAME = 256 * 1024;
const write = process.stdout.write.bind(process.stdout);
console.log = console.info = console.debug = (...args) => console.error(...args);
const imported = await import(pathToFileURL(process.argv[2]).href);
const project = imported.default ?? imported;
const context = {
  context: null,
  values: {},
  definition: null,
  emit(event, correlation = this.context) {
    const line = JSON.stringify({ context: correlation, event }) + '\n';
    if (Buffer.byteLength(line) > MAX_FRAME) throw new Error('worker output exceeds frame budget');
    if (process.stdout.writableLength + Buffer.byteLength(line) > MAX_FRAME * 2) {
      throw new Error('worker output backpressure budget exceeded');
    }
    write(line);
  },
  publish(patch) { this.emit({ type: 'patch', patch }); },
};

async function dispatch(message) {
  const { event, context: correlation } = message;
  const kind = event.type;
  context.context = correlation;
  if (kind === 'stop') { process.exit(0); }
  if (kind === 'init') {
    context.definition = event.definition;
    context.values = { ...event.values };
  } else if (kind === 'input') {
    context.values[event.key] = event.value;
  } else if (kind === 'action') {
    context.values = { ...event.values };
  }
  try {
    const callback = project['on_' + kind];
    const patch = callback ? await callback(context, event) : undefined;
    if (kind === 'action') {
      context.emit({ type: 'result', patch: patch ?? {} }, correlation);
    } else {
      if (patch !== undefined) context.publish(patch);
      context.emit({ type: kind === 'init' ? 'ready' : 'ack' }, correlation);
    }
  } catch (error) {
    context.emit({ type: 'error', message: String(error).slice(0, 4096) }, correlation);
  }
}

let buffer = Buffer.alloc(0);
let pending = 0;
let chain = Promise.resolve();
process.stdin.on('data', chunk => {
  buffer = Buffer.concat([buffer, chunk]);
  let newline;
  while ((newline = buffer.indexOf(10)) >= 0) {
    if (newline + 1 > MAX_FRAME || pending >= 64) process.exit(2);
    const line = buffer.subarray(0, newline).toString('utf8');
    buffer = buffer.subarray(newline + 1);
    pending += 1;
    chain = chain.then(() => dispatch(JSON.parse(line))).finally(() => { pending -= 1; });
    chain.catch(error => { console.error(error); process.exit(2); });
  }
  if (buffer.length >= MAX_FRAME) process.exit(2);
});
process.stdin.on('end', () => {
  chain.finally(() => process.exit(buffer.length ? 2 : 0));
});
