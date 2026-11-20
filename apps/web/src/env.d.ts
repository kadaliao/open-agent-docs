/// <reference types="vite/client" />
declare module '@open-agent-docs/markdown' {
  export const available: boolean | undefined
  export function parse(md: string): unknown
  export function serialize(doc: unknown): string
  const init: (() => Promise<unknown>) | undefined
  export default init
}
