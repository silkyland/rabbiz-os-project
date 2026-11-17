import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
const context = vm.createContext({});
vm.runInContext(readFileSync(new URL('../A21s/Desktop/AppCatalog.js', import.meta.url), 'utf8').replace(/^\.pragma library\s*/, ''), context);
const entry = (id, extra = {}) => ({ id, name: id, ...extra });
const ids = entries => Array.from(context.visibleEntries(entries), e => e.id);
// Third-party identity wins: Chrome must not be replaced by the a21s wrapper.
assert.deepEqual(ids([entry('google-chrome'), entry('a21s-browser'), entry('com.google.Chrome')]), ['google-chrome']);
assert.deepEqual(ids([entry('a21s-browser')]), ['a21s-browser']);
assert.deepEqual(ids([entry('org.kde.index'), entry('a21s-files')]), ['org.kde.index']);
assert.deepEqual(ids([entry('a21s-files')]), ['a21s-files']);
assert.deepEqual(ids([entry('org.kde.calindori'), entry('org.kde.merkuro.calendar')]), ['org.kde.merkuro.calendar']);
assert.deepEqual(ids([entry('a21s-photos'), entry('a21s-gallery', { noDisplay: true }), entry('org.kde.pix')]), ['a21s-photos']);
assert.deepEqual(ids([entry('a21s-terminal', { noDisplay: true }), entry('xterm'), entry('org.kde.station')]), ['org.kde.station']);
assert.deepEqual(ids([entry('a21s-notes', { hidden: true }), entry('org.kde.buho')]), ['org.kde.buho']);
assert.deepEqual(ids([null, entry(''), entry('ai.opencode'), entry('ai.opencode'), entry('other', { name: 'ai.opencode' })]), ['ai.opencode', 'other']);
assert.deepEqual(ids([]), []);
const native = ['a21s-clock', 'a21s-calculator', 'a21s-notes', 'a21s-music', 'a21s-terminal', 'a21s-photos'];
assert.deepEqual(ids(native.map(id => entry(id)).concat(['org.kde.kclock', 'org.kde.kalk', 'org.kde.marknote', 'org.kde.elisa', 'org.kde.qmlkonsole', 'org.kde.koko'].map(id => entry(id)))), native);
// Launcher/picker view: helpers are not apps, search matches names, order is by name.
const names = list => Array.from(list, e => e.name);
assert.deepEqual(Array.from(context.launcherEntries([
    entry('org.kde.kiod6'), entry('kcm_bad'), entry('foo-geo-handler'), entry('a21s-clock'), entry('xterm')
]), e => e.id), ['a21s-clock', 'xterm']);
assert.deepEqual(Array.from(context.launcherEntries([
    entry('a21s-clock', { name: 'Clock' }), entry('a21s-calculator', { name: 'Calc' }), entry('a21s-notes', { name: 'Notes' })
], 'l'), e => e.name), ['Calc', 'Clock']);
assert.deepEqual(Array.from(context.launcherEntries([entry('a'), entry('b')], '   '), e => e.id), ['a', 'b']);
assert.deepEqual(names(context.sortByName([entry('b', { name: 'Bravo' }), entry('a', { name: 'Alpha' })])), ['Alpha', 'Bravo']);
assert.equal(context.matchesQuery(entry('x', { name: 'Google Chrome' }), 'chrome'), true);
assert.equal(context.matchesQuery(entry('x', { name: 'Google Chrome' }), 'firefox'), false);
console.log('app catalog: preferred apps, fallbacks, hidden entries and duplicate IDs: ok');
