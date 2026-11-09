/** The app's base path without its trailing slash: empty on the server, `/upheap` for the local build on Pages. */
export const BASE = import.meta.env.BASE_URL.replace(/\/$/, "");
