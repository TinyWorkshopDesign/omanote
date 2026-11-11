import { isCleartextRemote } from "../src/lib/serverUrl.ts";
import assert from "node:assert/strict";

const warn = [
  "http://joplin.example.com",
  "http://192.168.1.10:22300",
  "HTTP://Joplin.Example.com/",
  "  http://10.0.0.2:22300/  ",
  "http://nas.local:22300",
  "http://[fe80::1]:22300",
  "http://127.example.com",
  "http://localhost.example.com",
  "http://[::2]",
];
const fine = [
  "https://joplin.example.com",
  "https://192.168.1.10:22300",
  "http://localhost:22300",
  "http://LOCALHOST",
  "http://localhost.:22300",
  "http://joplin.localhost:22300",
  "http://127.0.0.1:22300",
  "http://127.1.2.3",
  "http://[::1]:22300",
  "http://[0:0:0:0:0:0:0:1]:22300",
  "",
  "http://",
  "joplin.example.com",
];
for (const u of warn) assert.equal(isCleartextRemote(u), true, `should warn: ${u}`);
for (const u of fine) assert.equal(isCleartextRemote(u), false, `should not warn: ${u}`);
console.log(`serverUrl: ${warn.length + fine.length} cases ok`);
