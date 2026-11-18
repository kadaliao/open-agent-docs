/* tslint:disable */
/* eslint-disable */

export class RawEngine {
    free(): void;
    [Symbol.dispose](): void;
    apply(changes: string): string;
    checkFormula(src: string, key?: string | null): string;
    computed(row: string): string;
    displayRow(row: string): string;
    display(key: string, value: string): string;
    loadRelated(db: string, schema: string, rows: string): void;
    loadRows(rows: string): void;
    /**
     * `schema`、`opts`（`{ id?, now?, tz?, people? }`）都是 JSON。
     */
    constructor(schema: string, opts: string);
    normalize(key: string, input: string): string;
    query(view: string): string;
    schemaErrors(): string;
    /**
     * 固定时钟（毫秒时间戳、时区分钟数）；之后不再跟着浏览器走。
     */
    setClock(now_ms: number, tz: number): void;
    setPeople(people: string): void;
    setSchema(schema: string): void;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_rawengine_free: (a: number, b: number) => void;
    readonly rawengine_apply: (a: number, b: number, c: number) => [number, number, number, number];
    readonly rawengine_checkFormula: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly rawengine_computed: (a: number, b: number, c: number) => [number, number];
    readonly rawengine_display: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly rawengine_displayRow: (a: number, b: number, c: number) => [number, number];
    readonly rawengine_loadRelated: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number];
    readonly rawengine_loadRows: (a: number, b: number, c: number) => [number, number];
    readonly rawengine_new: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly rawengine_normalize: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly rawengine_query: (a: number, b: number, c: number) => [number, number, number, number];
    readonly rawengine_schemaErrors: (a: number) => [number, number];
    readonly rawengine_setClock: (a: number, b: number, c: number) => void;
    readonly rawengine_setPeople: (a: number, b: number, c: number) => [number, number];
    readonly rawengine_setSchema: (a: number, b: number, c: number) => [number, number];
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
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
