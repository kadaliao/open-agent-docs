/* tslint:disable */
/* eslint-disable */

/**
 * 只读 HTML 的样式表（和服务器 `/embed/content.css`、包里的 `content.css` 同一份）。
 */
export function contentCss(): string;

/**
 * Markdown → 文档（ProseMirror JSON 字符串）。
 */
export function parse(md: string): string;

/**
 * 文档（JSON 字符串）→ Markdown。
 */
export function serialize(doc: string): string;

/**
 * 一块 → Markdown（复制一块时用）。
 */
export function serializeBlock(block: string): string;

/**
 * Markdown（字符串）或文档（ProseMirror JSON 对象）→ 只读 HTML，和服务器 `?format=html` 逐字一样。
 * `options.baseUrl`：`/` 开头的地址（上传的文件、页面链接）前面拼上它。
 */
export function toHtml(input: string | object, options?: { baseUrl?: string }): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly contentCss: () => [number, number];
    readonly parse: (a: number, b: number) => [number, number];
    readonly serialize: (a: number, b: number) => [number, number, number, number];
    readonly serializeBlock: (a: number, b: number) => [number, number, number, number];
    readonly toHtml: (a: any, b: number) => [number, number, number, number];
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
