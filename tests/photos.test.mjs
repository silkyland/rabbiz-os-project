// node --test tests/photos.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const raw = readFileSync(new URL("../apps/photos/PhotosLogic.js", import.meta.url), "utf8");
// `.pragma library` is a QML directive, not JavaScript.
const src = raw.split("\n").filter((l) => !l.trimStart().startsWith(".pragma")).join("\n");
const ctx = { console };
vm.createContext(ctx);
vm.runInContext(
  src +
    "\nthis.Photos = { IMAGE_EXTENSIONS, ZOOM_MIN, ZOOM_MAX, extensionOf, isImagePath," +
    " baseName, dirName, findArgs, parseFindOutput, fileUrl, formatDate, formatCount," +
    " displayFolder, wrapIndex, nextIndex, prevIndex, clampZoom, zoomTarget, isZoomed," +
    " maxPan, clampPan, isSwipe };",
  ctx,
);
const P = ctx.Photos;

test("extensions are matched case-insensitively and dotfiles are not images", () => {
  assert.deepEqual(Array.from(P.IMAGE_EXTENSIONS), ["jpg", "jpeg", "png", "webp"]);
  assert.equal(P.isImagePath("/home/g/Pictures/a.jpg"), true);
  assert.equal(P.isImagePath("/home/g/Pictures/a.JPEG"), true);
  assert.equal(P.isImagePath("/home/g/Pictures/a.Png"), true);
  assert.equal(P.isImagePath("/home/g/Downloads/a.webp"), true);
  assert.equal(P.isImagePath("/home/g/Pictures/a.gif"), false);
  assert.equal(P.isImagePath("/home/g/Pictures/a"), false);
  assert.equal(P.isImagePath("/home/g/Pictures.jpg/a"), false);
  assert.equal(P.isImagePath("/home/g/Pictures/.jpg"), false);
  assert.equal(P.isImagePath(""), false);
  assert.equal(P.extensionOf("/home/g/Pictures/a.tar.JPG"), "jpg");
  assert.equal(P.baseName("/home/g/Pictures/a b.png"), "a b.png");
  assert.equal(P.dirName("/home/g/Pictures/2024/c.jpeg"), "/home/g/Pictures/2024");
  assert.equal(P.dirName("/a.png"), "/");
  assert.equal(P.dirName("a.png"), "");
});

test("find argv is shell-free and only lists images", () => {
  assert.deepEqual(Array.from(P.findArgs(["/home/g/Pictures", "/home/g/Downloads"])), [
    "find", "/home/g/Pictures", "/home/g/Downloads",
    "-type", "f", "(",
    "-iname", "*.jpg", "-o", "-iname", "*.jpeg", "-o", "-iname", "*.png",
    "-o", "-iname", "*.webp", ")",
    "-printf", "%T@\\t%p\\n",
  ]);
  assert.deepEqual(Array.from(P.findArgs([])), []);
  assert.deepEqual(Array.from(P.findArgs(["/home/g/Pictures", ""])), [
    "find", "/home/g/Pictures",
    "-type", "f", "(",
    "-iname", "*.jpg", "-o", "-iname", "*.jpeg", "-o", "-iname", "*.png",
    "-o", "-iname", "*.webp", ")",
    "-printf", "%T@\\t%p\\n",
  ]);
});

test("find output is sorted newest first and bad lines are skipped", () => {
  const out = [
    "1763282499.0\t/home/rabbiz/Pictures/b.jpg",
    "1763196000.5\t/home/rabbiz/Pictures/a.png",
    "1763282499.0\t/home/rabbiz/Pictures/2024/c.jpeg",
    "1763282600\t/home/rabbiz/Downloads/d.WEBP",
    "1763282499\t/home/rabbiz/Pictures/skip.gif",
    "1763282499\t/home/rabbiz/Pictures/.jpg",
    "1763282499\t/home/rabbiz/Pictures/b.jpg",
    "/home/rabbiz/Pictures/e.png",
    "1763282499\t/home/rabbiz/Pictures/f.png\r",
    "",
  ].join("\n");

  const photos = P.parseFindOutput(out);
  assert.deepEqual(Array.from(photos, (p) => p.name), [
    "d.WEBP", "c.jpeg", "b.jpg", "f.png", "a.png", "e.png",
  ]);
  assert.equal(photos[0].path, "/home/rabbiz/Downloads/d.WEBP");
  assert.equal(photos[0].mtime, 1763282600000);
  assert.equal(photos[1].dir, "/home/rabbiz/Pictures/2024");
  assert.equal(photos[4].mtime, 1763196000500); // unix seconds -> ms
  assert.equal(photos[5].mtime, 0);             // find fallback without %T@
});

test("empty and malformed find output yields no photos", () => {
  assert.equal(P.parseFindOutput("").length, 0);
  assert.equal(P.parseFindOutput(undefined).length, 0);
  assert.equal(P.parseFindOutput("\n\n").length, 0);
  assert.equal(P.parseFindOutput("1763282499\t/home/g/notes.txt").length, 0);
});

test("file urls encode spaces and Thai names", () => {
  assert.equal(P.fileUrl("/home/g/Pictures/my photo.png"),
    "file:///home/g/Pictures/my%20photo.png");
  assert.equal(P.fileUrl("/home/rabbiz/Pictures/รูป.png"),
    "file:///home/rabbiz/Pictures/%E0%B8%A3%E0%B8%B9%E0%B8%9B.png");
  assert.equal(P.fileUrl("/home/g/a#b%c.png"), "file:///home/g/a%23b%25c.png");
  assert.equal(P.fileUrl("file:///already/encoded.png"), "file:///already/encoded.png");
  assert.equal(P.fileUrl(""), "");
});

test("dates read as today / yesterday / this year / older", () => {
  const now = new Date(2026, 10, 16, 18, 30).getTime();
  assert.equal(P.formatDate(new Date(2026, 10, 16, 14, 41).getTime(), now), "Today, 14:41");
  assert.equal(P.formatDate(new Date(2026, 10, 15, 9, 5).getTime(), now), "Yesterday, 09:05");
  assert.equal(P.formatDate(new Date(2026, 0, 2, 3, 4).getTime(), now), "2 Jan, 03:04");
  assert.equal(P.formatDate(new Date(2025, 11, 31, 23, 59).getTime(), now), "31 Dec 2025, 23:59");
  assert.equal(P.formatDate(0, now), "");
  assert.equal(P.formatDate(undefined, now), "");
});

test("count copy never reads as a raw number", () => {
  assert.equal(P.formatCount(0), "No photos");
  assert.equal(P.formatCount(1), "1 photo");
  assert.equal(P.formatCount(2), "2 photos");
  assert.equal(P.formatCount(12), "12 photos");
  assert.equal(P.formatCount(NaN), "No photos");
});

test("folders are shortened against the home directory", () => {
  assert.equal(P.displayFolder("/home/rabbiz/Pictures/2024", "/home/rabbiz"), "~/Pictures/2024");
  assert.equal(P.displayFolder("/home/rabbiz", "/home/rabbiz"), "~");
  assert.equal(P.displayFolder("/media/sdcard/DCIM", "/home/rabbiz"), "/media/sdcard/DCIM");
  assert.equal(P.displayFolder("/home/rabbiz/Pictures", ""), "/home/rabbiz/Pictures");
});

test("index wrap never dead-ends", () => {
  assert.equal(P.wrapIndex(5, 3), 2);
  assert.equal(P.wrapIndex(-1, 3), 2);
  assert.equal(P.wrapIndex(0, 1), 0);
  assert.equal(P.wrapIndex(9, 3), 0);
  assert.equal(P.wrapIndex(2, 5), 2);
  assert.equal(P.wrapIndex(0, 0), -1);
  assert.equal(P.wrapIndex(4, 0), -1);
  assert.equal(P.wrapIndex(0, -3), -1);
  assert.equal(P.nextIndex(4, 5), 0);
  assert.equal(P.prevIndex(0, 5), 4);
  assert.equal(P.prevIndex(0, 0), -1);
});

test("zoom toggles between 1x and 2.5x, clamped", () => {
  assert.equal(P.ZOOM_MIN, 1.0);
  assert.equal(P.ZOOM_MAX, 2.5);
  assert.equal(P.zoomTarget(1), 2.5);
  assert.equal(P.zoomTarget(1.0), 2.5);
  assert.equal(P.zoomTarget(2.5), 1.0);
  assert.equal(P.zoomTarget(1.7), 1.0);
  assert.equal(P.zoomTarget(0), 2.5);
  assert.equal(P.zoomTarget(9), 1.0);
  assert.equal(P.clampZoom(0.5, 1, 2.5), 1);
  assert.equal(P.clampZoom(4, 1, 2.5), 2.5);
  assert.equal(P.clampZoom(1.7, 1, 2.5), 1.7);
  assert.equal(P.clampZoom(NaN, 1, 2.5), 1);
  assert.equal(P.clampZoom(2, 3, 1), 3);
  assert.equal(P.isZoomed(1), false);
  assert.equal(P.isZoomed(1.001), false);
  assert.equal(P.isZoomed(2.5), true);
  assert.equal(P.isZoomed(NaN), false);
});

test("pan room follows the visible overflow only", () => {
  assert.equal(P.maxPan(700, 700, 1), 0);
  assert.equal(P.maxPan(700, 700, 2.5), 525);
  assert.equal(P.maxPan(1400, 700, 2.5), 1400);
  assert.equal(P.maxPan(0, 700, 2.5), 0);
  assert.equal(P.maxPan(700, 700, 0.5), 0);
  assert.equal(P.maxPan(NaN, 700, 2), 0);
  assert.equal(P.clampPan(100, 50), 50);
  assert.equal(P.clampPan(-100, 50), -50);
  assert.equal(P.clampPan(20, 50), 20);
  assert.equal(P.clampPan(NaN, 50), 0);
  assert.equal(P.clampPan(20, NaN), 0);
  assert.equal(P.clampPan(-20, -50), -20);
});

test("only a long, mostly horizontal drag counts as a swipe", () => {
  assert.equal(P.isSwipe(-120, 10, 72), true);
  assert.equal(P.isSwipe(120, -30, 72), true);
  assert.equal(P.isSwipe(40, 0, 72), false);
  assert.equal(P.isSwipe(-120, 110, 72), false);
  assert.equal(P.isSwipe(-120, NaN, 72), true);
  assert.equal(P.isSwipe(NaN, 0, 72), false);
});
