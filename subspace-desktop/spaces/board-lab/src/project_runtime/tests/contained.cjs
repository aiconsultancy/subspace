const semver = require('semver');
const sibling = require('./sibling.cjs');
let count = 0;

function result(text) {
  return { results: { heading: {
    scope: 'synthetic-local-contained', evaluated_at_ms: 1234,
    value: { type: 'text', text },
  } } };
}

module.exports = {
  on_init(context) {
    if (process.env.SYNTHETIC_VALUE !== 'dummy-test-only-selected-value') throw new Error('missing selected value');
    if (process.env.SUBSPACE_RUNTIME_TEST_ROOT !== undefined) throw new Error('inherited host environment');
    if (semver.major('1.2.3') !== 1 || sibling.VALUE !== 7) throw new Error('ordinary module import failed');
    console.log(process.env.SYNTHETIC_VALUE);
    setTimeout(() => context.publish(result('timer')), 50);
  },
  on_action(context, event) {
    if (event.action === 'malformed') {
      process.stdout.write('not-json\n');
      return {};
    }
    return result(`count:${++count}:${process.pid}`);
  },
};
