import { serve } from '@hono/node-server'
import { Hono } from 'hono'
import { loadEnv } from './env.js'

const env = loadEnv()
const app = new Hono()

app.get('/health', (c) => c.json({ ok: true }))

serve({ fetch: app.fetch, port: env.PORT })
