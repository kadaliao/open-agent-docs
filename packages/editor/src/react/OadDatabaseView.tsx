// <OadDatabaseView>：数据库视图的按需加载版（第一次用到时才下载表格、看板这些代码和样式）。
// 想直接打进包里就 import { DatabaseView } from '@open-agent-docs/editor/database'。
import { lazy, Suspense } from 'react'
import type { DatabaseViewProps } from '../database/DatabaseView'
import { useOadClient } from './context'

const Lazy = lazy(() => import('../database/index').then((m) => ({ default: m.DatabaseView })))

export type OadDatabaseViewProps = Omit<DatabaseViewProps, 'client'> & { client?: DatabaseViewProps['client'] }

export function OadDatabaseView(p: OadDatabaseViewProps) {
  const client = useOadClient(p.client)
  return (
    <Suspense fallback={<div className="db db-skeleton" aria-busy="true" style={{ minHeight: 120 }} />}>
      <Lazy {...p} client={client} />
    </Suspense>
  )
}
