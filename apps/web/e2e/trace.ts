// 读 Chromium 的性能 trace，看渲染主线程上每个任务花了多少墙上时间、多少 CPU 时间。
// 机器很忙时，系统会把主线程挂起几百毫秒：长任务的墙上时间很长，CPU 时间却只有几毫秒——那不是页面代码慢。
// 两个分开看，才知道一次卡顿该不该怪代码。
import { readFileSync } from 'node:fs'
import type { Browser, Page } from '@playwright/test'

/** 要的类别：toplevel 有每个任务（带 tdur = 线程 CPU 时间），devtools.timeline 有 Paint / Layout，cc 有每一帧 */
export const TRACE_CATEGORIES = ['toplevel', 'devtools.timeline', 'blink', 'cc', 'v8']

export interface MainThreadStats {
  /** 每一帧主线程的活（BeginMainFrame：样式、布局、绘制）的 CPU 时间 */
  frameCpu: { n: number; p50: number; p95: number; max: number }
  /** 墙上时间超过 50ms 的任务：各自的墙上时间、CPU 时间、主要花在哪 */
  longTasks: { wall: number; cpu: number; top: string }[]
  /** CPU 时间超过 50ms 的任务数（这才是代码的问题） */
  cpuLong: number
  /** 墙上时间超过 50ms、但 CPU 时间不到一半的（线程被系统挂起了） */
  preempted: number
}

const q = (a: number[], p: number) => (a.length ? +a[Math.min(a.length - 1, Math.floor(p * a.length))].toFixed(1) : 0)

export function mainThreadStats(path: string): MainThreadStats {
  const raw = JSON.parse(readFileSync(path, 'utf8'))
  const ev: any[] = raw.traceEvents ?? raw
  const names = new Map<string, string>()
  for (const e of ev) if (e.ph === 'M' && e.name === 'thread_name') names.set(`${e.pid}:${e.tid}`, e.args.name)
  const mains = new Set([...names].filter(([, n]) => n === 'CrRendererMain').map(([k]) => k))
  const onMain = ev.filter((e) => e.ph === 'X' && mains.has(`${e.pid}:${e.tid}`))
  const frames = onMain.filter((e) => e.name === 'ProxyMain::BeginMainFrame')
  const cpu = frames.map((e) => (e.tdur ?? e.dur) / 1000).sort((a, b) => a - b)
  const tasks = onMain.filter((e) => (e.name === 'ThreadControllerImpl::RunTask' || e.name === 'RunTask') && e.dur > 50_000)
  const longTasks = tasks.map((t) => {
    // 任务里最大的几个事件（看是 Paint、Layout、脚本还是 GC）
    const inner = onMain.filter((e) => e !== t && e.ts >= t.ts && e.ts + e.dur <= t.ts + t.dur && e.dur > t.dur * 0.3 && !/RunTask|BeginMainFrame|UpdateLifecycle|UpdateTime|LifecyclePhase/.test(e.name))
    return { wall: +(t.dur / 1000).toFixed(1), cpu: +((t.tdur ?? t.dur) / 1000).toFixed(1), top: [...new Set(inner.map((e) => e.name))].slice(0, 3).join(', ') }
  })
  return {
    frameCpu: { n: cpu.length, p50: q(cpu, 0.5), p95: q(cpu, 0.95), max: q(cpu, 1) },
    longTasks,
    cpuLong: longTasks.filter((t) => t.cpu > 50).length,
    preempted: longTasks.filter((t) => t.cpu < t.wall / 2).length,
  }
}

/** 在 trace 里跑 fn，返回 fn 的结果和主线程的统计 */
export async function traced<T>(browser: Browser, page: Page, path: string, fn: () => Promise<T>): Promise<{ result: T; main: MainThreadStats }> {
  await browser.startTracing(page, { path, categories: TRACE_CATEGORIES })
  const result = await fn()
  await browser.stopTracing()
  return { result, main: mainThreadStats(path) }
}
