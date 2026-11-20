// @open-agent-docs/editor/database：数据库视图（表格、看板、日历、画廊、列表）、行的属性头。单独一个入口，不用的宿主不下载。
import '../react/base.css'
import './db.css'
export { DatabaseView, DatabaseView as OadDatabaseView, useDbPeople, type DatabaseViewProps } from './DatabaseView'
export { RowHeader, RowProps } from './RowPanel'
export { DbContext, useDb, type DbCtx } from './cells'
export { readDbSnapshot, saveDbSnapshot, takeDbSnapshot } from './snapshot'
export { t as dbT } from './strings'
