// Proves the shade's grouping transform: categories, per-app collapse, ordering, dim flag.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const src = readFileSync(new URL("../Rabbiz/Panels/NotificationGroups.js", import.meta.url), "utf8")
    .replace(/^\.pragma library\s*$/m, "");
const box = { module: { exports: {} } };
vm.runInNewContext(src, box);
const g = box.module.exports;

const notif = (appName, summary, body = "") => ({ appName, summary, body });
const cardsOf = sections => sections.reduce((all, s) => all.concat(s.cards), []);
const categories = sections => sections.map(s => s.category).join(",");

// empty input renders nothing (the sheet shows its own empty state)
assert.equal(g.build([]).length, 0);
assert.equal(g.build(null).length, 0);

// one app group per app, newest first inside the group
const messenger = [
    notif("Messenger", "Winura: old", "first"),
    notif("Messenger", "Winura: new", "second")
];
let sections = g.build(messenger);
assert.equal(cardsOf(sections).length, 1, "same app collapses into one card");
assert.equal(cardsOf(sections)[0].count, 2);
assert.equal(cardsOf(sections)[0].items[0].summary, "Winura: new", "newest first");
assert.equal(sections.length, 1);
assert.equal(sections[0].title, "Conversations");

// category order is conversations -> devices -> apps -> promotions, whatever the arrival order
sections = g.build([
    notif("Shopee", "12.12 sale"),
    notif("Random Tool", "Index done"),
    notif("Device share", "Sharing as Wi-Fi hotspot"),
    notif("Telegram", "Ada: hi")
]);
assert.equal(categories(sections), "conversations,devices,apps,promotions");

// promo senders are promotions even with a neutral body; cards there are dimmed
const promo = cardsOf(sections).find(c => c.category === "promotions");
assert.equal(promo.key, "shopee");
assert.equal(promo.dim, true);
assert.equal(cardsOf(sections).find(c => c.category === "conversations").dim, false);

// promo wording in the body promotes an unknown app, but never a chat app
assert.equal(g.categoryOf("AIS", "promo"), "promotions", "known marketing sender");
assert.equal(g.categoryOf("Some Shop", "ลด 50% ส่วนลดวันนี้"), "promotions", "promo wording");
assert.equal(g.categoryOf("Messenger", "ส่งส่วนลดให้เพื่อน"), "conversations",
    "a friend typing 'ส่วนลด' is still a conversation");

// device/system senders
assert.equal(g.categoryOf("Bluetooth", "Galaxy Watch4 connected"), "devices");
assert.equal(g.categoryOf("", "something"), "apps", "empty app name is not a device");
assert.equal(g.categoryOf("Random Tool", "done"), "apps");

// missing app name still groups, under "System"
const unnamed = cardsOf(g.build([notif("", "hello"), notif("", "world")]));
assert.equal(unnamed.length, 1);
assert.equal(unnamed[0].appName, "System");
assert.equal(unnamed[0].count, 2);

// section counts are notification counts, not group counts
sections = g.build([
    notif("Telegram", "a"), notif("Telegram", "b"), notif("SMS", "c"),
    notif("Shopee", "d")
]);
const conversations = sections.find(s => s.category === "conversations");
assert.equal(conversations.count, 3);
assert.equal(conversations.cards.length, 2, "two chat apps stay two cards");

// every card carries the fields the delegate reads
const card = cardsOf(sections)[0];
for (const field of ["key", "appName", "items", "count", "category", "dim"])
    assert.ok(field in card, `card exposes ${field}`);
for (const field of ["category", "title", "count", "cards"])
    assert.ok(field in sections[0], `section exposes ${field}`);

console.log("notification groups: ok");
