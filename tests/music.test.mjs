// node --test tests/music.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const raw = readFileSync(new URL("../apps/music/MusicLogic.js", import.meta.url), "utf8");
// `.pragma library` is a QML directive, not JavaScript (same trick as tests/calc.test.mjs).
const src = raw.split("\n").filter((l) => !l.trimStart().startsWith(".pragma")).join("\n");
const ctx = { console };
vm.createContext(ctx);
vm.runInContext(
  src +
    "\nthis.Music = { parseFind, parseProbe, parseDuration, titleFromPath, baseName, extensionOf, " +
    "formatTime, nextIndex, prevIndex, autoNextIndex, indexOfPath, countFolder, filterTracks, " +
    "progressFraction, seekSeconds, AUDIO_EXTENSIONS };",
  ctx
);
const Music = ctx.Music;

const HOME = "/home/gamer";

// Exactly what the app's `find ... -printf '%T@\t%p\n' | sort -rn` produces.
const FIND_OUTPUT = [
  "1763500000.25\t/home/gamer/Downloads/Podcast_Episode_12.opus",
  "1763500000.25\t/home/gamer/Downloads/Podcast_Episode_12.opus",
  "1763400000.5\t/home/gamer/Music/Albums/Blue/01 Awakening.flac",
  "1763300000.0\t/home/gamer/Music/มืดมน.mp3",
  "1763200000.0\t/home/gamer/Downloads/cover.jpg",
  "1763100000.0\t/home/gamer/Music/Sub/Dir/Deep Track.ogg",
  "1763000000.0\t/home/gamer/Music/notes.txt",
  "1762900000.0\t/home/gamer/Music/loop tap.wav",
].join("\n");

test("find output becomes a newest-first track list", () => {
  const tracks = Music.parseFind(FIND_OUTPUT, HOME);
  // Spread into a host-realm array: vm results have a different Array prototype.
  assert.deepEqual(
    [...tracks.map((t) => t.path)],
    [
      "/home/gamer/Downloads/Podcast_Episode_12.opus",
      "/home/gamer/Music/Albums/Blue/01 Awakening.flac",
      "/home/gamer/Music/มืดมน.mp3",
      "/home/gamer/Music/Sub/Dir/Deep Track.ogg",
      "/home/gamer/Music/loop tap.wav",
    ]
  );
});

test("titles, folders and captions", () => {
  const tracks = Music.parseFind(FIND_OUTPUT, HOME);
  const byPath = Object.fromEntries(tracks.map((t) => [t.path, t]));

  const podcast = byPath["/home/gamer/Downloads/Podcast_Episode_12.opus"];
  assert.equal(podcast.title, "Podcast Episode 12");
  assert.equal(podcast.folder, "Downloads");
  assert.equal(podcast.subfolder, "");
  assert.equal(podcast.caption, "Downloads");
  assert.equal(podcast.ext, "opus");

  const flac = byPath["/home/gamer/Music/Albums/Blue/01 Awakening.flac"];
  assert.equal(flac.title, "01 Awakening");
  assert.equal(flac.folder, "Music");
  assert.equal(flac.subfolder, "Albums/Blue");
  assert.equal(flac.caption, "Music · Albums/Blue");

  // Thai filenames survive the round trip.
  const thai = byPath["/home/gamer/Music/มืดมน.mp3"];
  assert.equal(thai.title, "มืดมน");
  assert.equal(thai.caption, "Music");

  // Duplicates, non-audio files and dot-directories of the same name are handled.
  assert.equal(tracks.filter((t) => t.path === podcast.path).length, 1);
  assert.equal(tracks.some((t) => t.ext === "jpg" || t.ext === "txt"), false);
});

test("parseFind survives junk and a path without a mtime prefix", () => {
  const tracks = Music.parseFind("\n\nnot-audio.txt\n\t\n/home/gamer/Music/Loose.WAV\n", HOME);
  assert.equal(tracks.length, 1);
  assert.equal(tracks[0].title, "Loose");
  assert.equal(tracks[0].ext, "wav");
  assert.equal(tracks[0].mtime, 0);
  assert.equal(Music.parseFind("", HOME).length, 0);
  assert.equal(Music.parseFind(null, HOME).length, 0);
});

test("extensionOf and titleFromPath", () => {
  assert.equal(Music.extensionOf("/a/b/Track.MP3"), "mp3");
  assert.equal(Music.extensionOf("/a/b/no-extension"), "");
  assert.equal(Music.extensionOf("/a/b/.hidden"), "");
  assert.equal(Music.baseName("/a/b/c.mp3"), "c.mp3");
  assert.equal(Music.baseName("c.mp3"), "c.mp3");
  assert.equal(Music.titleFromPath("/a/b/Final   _   Mix.flac"), "Final Mix");
  assert.equal(Music.titleFromPath("/a/b/_"), "Untitled");
});

test("parseProbe reads ffprobe key=value output", () => {
  const ok = Music.parseProbe(
    "filename=/home/gamer/Music/Albums/Blue/01 Awakening.flac\nduration=214.753333\n"
  );
  assert.equal(ok.path, "/home/gamer/Music/Albums/Blue/01 Awakening.flac");
  assert.equal(ok.duration, 214.753333);

  const unknown = Music.parseProbe("filename=/x/y.mp3\nduration=N/A\n");
  assert.equal(unknown.duration, 0);
  const empty = Music.parseProbe("");
  assert.equal(empty.path, "");
  assert.equal(empty.duration, 0);
  assert.equal(Music.parseDuration("-3"), 0);
  assert.equal(Music.parseDuration(""), 0);
});

test("formatTime", () => {
  assert.equal(Music.formatTime(0), "0:00");
  assert.equal(Music.formatTime(5), "0:05");
  assert.equal(Music.formatTime(59.9), "0:59");
  assert.equal(Music.formatTime(60), "1:00");
  assert.equal(Music.formatTime(214.75), "3:34");
  assert.equal(Music.formatTime(3599), "59:59");
  assert.equal(Music.formatTime(3600), "1:00:00");
  assert.equal(Music.formatTime(3661), "1:01:01");
  assert.equal(Music.formatTime(undefined), "--:--");
  assert.equal(Music.formatTime(-5), "--:--");
  assert.equal(Music.formatTime(NaN), "--:--");
});

test("explicit transport wraps, auto-advance stops at the end", () => {
  assert.equal(Music.nextIndex(0, 3), 1);
  assert.equal(Music.nextIndex(2, 3), 0);
  assert.equal(Music.nextIndex(-1, 3), 0);
  assert.equal(Music.nextIndex(0, 0), -1);

  assert.equal(Music.prevIndex(1, 3), 0);
  assert.equal(Music.prevIndex(0, 3), 2);
  assert.equal(Music.prevIndex(-1, 3), 0);
  assert.equal(Music.prevIndex(0, 0), -1);

  assert.equal(Music.autoNextIndex(0, 3), 1);
  assert.equal(Music.autoNextIndex(2, 3), -1);
  assert.equal(Music.autoNextIndex(-1, 3), 0);
  assert.equal(Music.autoNextIndex(0, 0), -1);
});

test("indexOfPath and folder counts", () => {
  const tracks = Music.parseFind(FIND_OUTPUT, HOME);
  assert.equal(Music.indexOfPath(tracks, "/home/gamer/Music/มืดมน.mp3"), 2);
  assert.equal(Music.indexOfPath(tracks, "/home/gamer/Music/gone.mp3"), -1);
  assert.equal(Music.indexOfPath([], "/x"), -1);
  assert.equal(Music.indexOfPath(null, "/x"), -1);

  assert.equal(Music.countFolder(tracks, "Music"), 4);
  assert.equal(Music.countFolder(tracks, "Downloads"), 1);
  assert.equal(Music.countFolder([], "Music"), 0);
});

test("filterTracks scopes the list without mutating it", () => {
  const tracks = Music.parseFind(FIND_OUTPUT, HOME);
  const music = Music.filterTracks(tracks, "music");
  const downloads = Music.filterTracks(tracks, "downloads");
  assert.equal(music.length, 4);
  assert.equal(downloads.length, 1);
  assert.equal(Music.filterTracks(tracks, "all").length, tracks.length);
  assert.equal(Music.filterTracks(tracks, "nonsense").length, tracks.length);
  assert.equal(tracks.length, 5);
  assert.notEqual(Music.filterTracks(tracks, "all"), tracks);
});

test("progress and seek are clamped and refuse an unknown length", () => {
  assert.equal(Music.progressFraction(30, 120), 0.25);
  assert.equal(Music.progressFraction(200, 120), 1);
  assert.equal(Music.progressFraction(-5, 120), 0);
  assert.equal(Music.progressFraction(10, 0), 0);
  assert.equal(Music.progressFraction(10, undefined), 0);

  assert.equal(Music.seekSeconds(0.5, 120), 60);
  assert.equal(Music.seekSeconds(0.5, 0), 0);
  assert.equal(Music.seekSeconds(-1, 120), 0);
  assert.equal(Music.seekSeconds(2, 120), 120);
  assert.equal(Music.seekSeconds(NaN, 120), 0);
});
