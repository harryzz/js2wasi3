export const meta = import.meta;
export function where() {
  return import.meta.url.slice(import.meta.url.lastIndexOf("/modules/"));
}
