// The server build's `fetch` for the API client (`#api-fetch`): none, so openapi-fetch uses the browser's. The local
// build resolves `#api-fetch` to src/local/fetch.ts instead (package.json `imports`, vite.config.ts).
export const apiFetch: ((request: Request) => Promise<Response>) | undefined = undefined;
