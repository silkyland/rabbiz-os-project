.pragma library
// NotificationGroups — pure grouping/classification for the shade's notification
// sheet (no Qt; tested with node in shell/tests/notificationgroups.test.mjs).
//
// One UI 7 files the pull-down into categories instead of one flat stream: the
// sheet reads top-down as people -> devices -> apps -> promotions, and every
// app's notifications collapse into one card with a count. This module owns
// that transform so the QML only renders rows.
//
// Category rules are deliberately simple and explainable (docs/a21s-phone-shell.md):
//   1. known marketing/promo senders (app name)          -> "promotions"
//   2. known chat apps                                   -> "conversations"
//   3. promo wording in the app name or the newest text   -> "promotions"
//   4. system/device apps                                -> "devices"
//   5. anything else                                     -> "apps"
// Chat apps win over promo wording on purpose: a friend typing "ส่วนลด" is a
// conversation, not an ad. Promo senders are listed first because marketplaces
// and banks otherwise look like generic "apps".
var CATEGORY_ORDER = ["conversations", "devices", "apps", "promotions"];
var CATEGORY_TITLES = {
    conversations: "Conversations",
    devices: "Devices",
    apps: "Apps",
    promotions: "Promotions"
};

var CHAT_APPS = [
    "messenger", "messages", "sms", "chatty", "whatsapp", "telegram", "signal",
    "element", "matrix", "discord", "slack", "line", "viber", "wechat",
    "instagram", "teams", "neochat", "tokodon", "kasts", "messaging"
];

var DEVICE_APPS = [
    "bluetooth", "bluetoothctl", "network", "nmcli", "wifi", "wi-fi", "upower",
    "system", "device", "share", "kde connect", "quickshell", "a21s", "battery",
    "power", "storage", "files", "settings", "modem", "telephony"
];

var PROMO_APPS = [
    "shopee", "lazada", "kbank", "scb", "ktc", "ttb", "bevfood", "grab",
    "foodpanda", "robinhood", "central", "ais", "true", "dtac", "advice"
];

var PROMO_WORDS = [
    "promo", "sale", "discount", "coupon", "voucher", "cashback", "offer",
    "deal", "% off", "ส่วนลด", "โปรโมชั่น", "โปรโมชัน", "คูปอง", "แลก",
    "รับเงิน", "ช้อป", "แถม", "สิทธิพิเศษ", "ฟรี", "ผ่อน"
];

function normalize(s) {
    return (s || "").toString().toLowerCase().trim();
}

function matchesAny(haystack, needles) {
    for (var i = 0; i < needles.length; ++i) {
        if (haystack.indexOf(needles[i]) >= 0)
            return true;
    }
    return false;
}

// body = newest summary + body (already joined by build()).
function categoryOf(appName, body) {
    var app = normalize(appName);
    var text = normalize(body);

    if (matchesAny(app, PROMO_APPS))
        return "promotions";
    if (matchesAny(app, CHAT_APPS))
        return "conversations";
    if (matchesAny(app, PROMO_WORDS) || matchesAny(text, PROMO_WORDS))
        return "promotions";
    if (matchesAny(app, DEVICE_APPS))
        return "devices";
    return "apps";
}

function sectionTitle(category) {
    return CATEGORY_TITLES[category] || CATEGORY_TITLES.apps;
}

// list: notifications in arrival order (oldest first), any object exposing
// appName / summary / body. Returns sections in display order, each holding the
// cards for that category:
//   [{ category, title, count, cards: [{ key, appName, items, count, dim }] }]
// `items` is newest-first and holds the original objects.
function build(list) {
    var source = list || [];
    var byApp = {};
    var order = [];

    for (var i = 0; i < source.length; ++i) {
        var n = source[i];
        if (!n)
            continue;
        var key = normalize(n.appName) || "system";
        if (!byApp[key]) {
            byApp[key] = [];
            order.push(key);
        }
        byApp[key].push({ item: n, index: i });
    }

    var groups = [];
    for (var g = 0; g < order.length; ++g) {
        var entries = byApp[order[g]];
        entries.sort(function (a, b) { return b.index - a.index; });   // newest first
        var items = [];
        for (var e = 0; e < entries.length; ++e)
            items.push(entries[e].item);
        var newest = items[0];
        var category = categoryOf(newest.appName, [newest.summary, newest.body].join(" "));
        groups.push({
            key: order[g],
            appName: (newest.appName || "").length > 0 ? newest.appName : "System",
            items: items,
            count: items.length,
            category: category,
            newest: entries[0].index,
            dim: category === "promotions"
        });
    }

    groups.sort(function (a, b) {
        var ca = CATEGORY_ORDER.indexOf(a.category);
        var cb = CATEGORY_ORDER.indexOf(b.category);
        if (ca !== cb)
            return ca - cb;
        return b.newest - a.newest;
    });

    var sections = [];
    var byCategory = {};
    for (var k = 0; k < groups.length; ++k) {
        var grp = groups[k];
        if (!byCategory[grp.category]) {
            byCategory[grp.category] = {
                category: grp.category,
                title: sectionTitle(grp.category),
                count: 0,
                cards: []
            };
            sections.push(byCategory[grp.category]);
        }
        byCategory[grp.category].cards.push(grp);
        byCategory[grp.category].count += grp.count;
    }
    return sections;
}

if (typeof module !== "undefined")
    module.exports = { build, categoryOf, sectionTitle, CATEGORY_ORDER, CATEGORY_TITLES };
