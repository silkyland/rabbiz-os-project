import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
const context = vm.createContext({});
vm.runInContext(readFileSync(new URL('../Rabbiz/Desktop/AppCatalog.js', import.meta.url), 'utf8').replace(/^\.pragma library\s*/, ''), context);
const entry = (id, extra = {}) => ({ id, name: id, ...extra });
const ids = entries => Array.from(context.visibleEntries(entries), e => e.id);
// Third-party identity wins: Chrome must not be replaced by the a21s wrapper.
assert.deepEqual(ids([entry('google-chrome'), entry('rabbiz-browser'), entry('com.google.Chrome')]), ['google-chrome']);
assert.deepEqual(ids([entry('rabbiz-browser')]), ['rabbiz-browser']);
assert.deepEqual(ids([entry('org.kde.index'), entry('rabbiz-files')]), ['org.kde.index']);
assert.deepEqual(ids([entry('rabbiz-files')]), ['rabbiz-files']);
assert.deepEqual(ids([entry('org.kde.calindori'), entry('org.kde.merkuro.calendar')]), ['org.kde.merkuro.calendar']);
assert.deepEqual(ids([entry('rabbiz-photos'), entry('rabbiz-gallery', { noDisplay: true }), entry('org.kde.pix')]), ['rabbiz-photos']);
assert.deepEqual(ids([entry('rabbiz-terminal', { noDisplay: true }), entry('xterm'), entry('org.kde.station')]), ['org.kde.station']);
assert.deepEqual(ids([entry('rabbiz-notes', { hidden: true }), entry('org.kde.buho')]), ['org.kde.buho']);
assert.deepEqual(ids([null, entry(''), entry('ai.opencode'), entry('ai.opencode'), entry('other', { name: 'ai.opencode' })]), ['ai.opencode', 'other']);
assert.deepEqual(ids([]), []);
const native = ['rabbiz-clock', 'rabbiz-calculator', 'rabbiz-notes', 'rabbiz-music', 'rabbiz-terminal', 'rabbiz-photos'];
assert.deepEqual(ids(native.map(id => entry(id)).concat(['org.kde.kclock', 'org.kde.kalk', 'org.kde.marknote', 'org.kde.elisa', 'org.kde.qmlkonsole', 'org.kde.koko'].map(id => entry(id)))), native);
// Launcher/picker view: helpers are not apps, search matches names, order is by name.
const names = list => Array.from(list, e => e.name);
assert.deepEqual(Array.from(context.launcherEntries([
    entry('org.kde.kiod6'), entry('kcm_bad'), entry('foo-geo-handler'), entry('rabbiz-clock'), entry('xterm')
]), e => e.id), ['rabbiz-clock', 'xterm']);
assert.deepEqual(Array.from(context.launcherEntries([
    entry('rabbiz-clock', { name: 'Clock' }), entry('rabbiz-calculator', { name: 'Calc' }), entry('rabbiz-notes', { name: 'Notes' })
], 'l'), e => e.name), ['Calc', 'Clock']);
assert.deepEqual(Array.from(context.launcherEntries([entry('a'), entry('b')], '   '), e => e.id), ['a', 'b']);
assert.deepEqual(names(context.sortByName([entry('b', { name: 'Bravo' }), entry('a', { name: 'Alpha' })])), ['Alpha', 'Bravo']);
assert.equal(context.matchesQuery(entry('x', { name: 'Google Chrome' }), 'chrome'), true);
assert.equal(context.matchesQuery(entry('x', { name: 'Google Chrome' }), 'firefox'), false);
console.log('app catalog: preferred apps, fallbacks, hidden entries and duplicate IDs: ok');
