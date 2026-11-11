/** True when a Joplin Server URL would carry the password (and, without E2EE, the
 * notes) in clear text over a network: plain `http://` to a host other than this
 * device's loopback. Not an error (servers on a home network are common), only a
 * reason to warn. Anything that does not parse as an http(s) URL gets no warning. */
export function isCleartextRemote(url: string): boolean {
  let u: URL;
  try {
    u = new URL(url.trim());
  } catch {
    return false;
  }
  if (u.protocol !== "http:") return false;
  return !isLoopback(u.hostname);
}

/** `localhost` (and `*.localhost`), 127.0.0.0/8 and `[::1]`: traffic never leaves the device. */
export function isLoopback(hostname: string): boolean {
  const h = hostname.toLowerCase().replace(/\.$/, "");
  return h === "localhost" || h.endsWith(".localhost") || h === "[::1]" || /^127(\.\d{1,3}){3}$/.test(h);
}
