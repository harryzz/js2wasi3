// Notes + todo on Express 5.
// Run:  node server.js [port]       (the port: the argument, else $PORT, else 3300)
// Or as a js2wasi3 component: see README.md. $APP_RUNTIME is only a label shown in the page.
'use strict'
const express = require('express')
const fs = require('node:fs')
const path = require('node:path')

const port = Number(process.env.PORT || process.argv[2] || 3300)
const runtime = process.env.APP_RUNTIME || 'node ' + process.version
const started = Date.now()
const dataDir = path.join(__dirname, 'data')
const dataFile = path.join(dataDir, 'db.json')
const COLORS = ['yellow', 'green', 'blue', 'pink', 'purple', 'gray']
const PRIORITIES = ['low', 'normal', 'high']

let db = { notes: [], todos: [], nextId: 1 }
try { db = JSON.parse(fs.readFileSync(dataFile, 'utf8')) } catch { /* first run: no file yet */ }
function save () {
  fs.mkdirSync(dataDir, { recursive: true })
  fs.writeFileSync(dataFile, JSON.stringify(db, null, 2))
}
if (db.todos.length === 0 && db.notes.length === 0) {
  // A first run starts with something to look at.
  const add = (list, item) => { item.id = db.nextId++; db[list].push(item) }
  add('todos', { title: 'Open this page in two browsers', done: false, priority: 'normal', due: '' })
  add('todos', { title: 'Tick me off', done: false, priority: 'high', due: '' })
  add('todos', { title: 'Compile Express to a WASI 0.3 component', done: true, priority: 'low', due: '' })
  add('notes', { title: 'Welcome', text: 'This page is served by Express. The server is a single WebAssembly component.', color: 'yellow', pinned: true, created: new Date().toISOString() })
  add('notes', { title: 'Shortcuts', text: '/ search  ·  t new todo  ·  n new note', color: 'blue', pinned: false, created: new Date().toISOString() })
  save()
}

let requests = 0
const app = express()
app.disable('x-powered-by')
app.use(express.json())
app.use((req, res, next) => { requests++; console.log(req.method, req.url); next() })
app.use(express.static(path.join(__dirname, 'public')))

const text = (v, max) => (typeof v === 'string' ? v.trim().slice(0, max) : '')
const matches = (q, ...fields) => !q || fields.some((f) => String(f || '').toLowerCase().includes(q))

// Todos: ?filter=open|done  ?q=text
app.get('/api/todos', (req, res) => {
  const q = text(req.query.q, 100).toLowerCase()
  const filter = req.query.filter
  res.json(db.todos.filter((t) => matches(q, t.title) && (filter === 'open' ? !t.done : filter === 'done' ? t.done : true)))
})
app.post('/api/todos', (req, res) => {
  const title = text(req.body && req.body.title, 200)
  if (!title) return res.status(400).json({ error: 'a todo needs a title' })
  const priority = PRIORITIES.includes(req.body.priority) ? req.body.priority : 'normal'
  const due = /^\d{4}-\d{2}-\d{2}$/.test(req.body.due || '') ? req.body.due : ''
  const todo = { id: db.nextId++, title, done: false, priority, due }
  db.todos.push(todo)
  save()
  res.status(201).json(todo)
})
app.patch('/api/todos/:id', (req, res) => {
  const todo = db.todos.find((t) => t.id === Number(req.params.id))
  if (!todo) return res.status(404).json({ error: 'not found' })
  const b = req.body || {}
  if (typeof b.done === 'boolean') todo.done = b.done
  if (typeof b.title === 'string' && text(b.title, 200)) todo.title = text(b.title, 200)
  if (PRIORITIES.includes(b.priority)) todo.priority = b.priority
  if (typeof b.due === 'string') todo.due = /^\d{4}-\d{2}-\d{2}$/.test(b.due) ? b.due : ''
  save()
  res.json(todo)
})
app.delete('/api/todos/:id', (req, res) => {
  const before = db.todos.length
  db.todos = db.todos.filter((t) => t.id !== Number(req.params.id))
  if (db.todos.length === before) return res.status(404).json({ error: 'not found' })
  save()
  res.status(204).end()
})
app.delete('/api/todos', (req, res) => { // ?done=1 clears the finished ones
  if (req.query.done !== '1') return res.status(400).json({ error: 'use ?done=1' })
  const n = db.todos.filter((t) => t.done).length
  db.todos = db.todos.filter((t) => !t.done)
  save()
  res.json({ removed: n })
})

// Notes: ?q=text, pinned first, newest first
app.get('/api/notes', (req, res) => {
  const q = text(req.query.q, 100).toLowerCase()
  const list = db.notes.filter((n) => matches(q, n.title, n.text))
  list.sort((a, b) => (b.pinned - a.pinned) || (a.created < b.created ? 1 : a.created > b.created ? -1 : 0))
  res.json(list)
})
app.post('/api/notes', (req, res) => {
  const b = req.body || {}
  if (!text(b.text, 2000)) return res.status(400).json({ error: 'a note needs text' })
  const note = {
    id: db.nextId++, title: text(b.title, 100), text: text(b.text, 2000),
    color: COLORS.includes(b.color) ? b.color : 'yellow', pinned: b.pinned === true, created: new Date().toISOString()
  }
  db.notes.push(note)
  save()
  res.status(201).json(note)
})
app.patch('/api/notes/:id', (req, res) => {
  const note = db.notes.find((n) => n.id === Number(req.params.id))
  if (!note) return res.status(404).json({ error: 'not found' })
  const b = req.body || {}
  if (typeof b.pinned === 'boolean') note.pinned = b.pinned
  if (COLORS.includes(b.color)) note.color = b.color
  if (typeof b.title === 'string') note.title = text(b.title, 100)
  if (typeof b.text === 'string' && text(b.text, 2000)) note.text = text(b.text, 2000)
  save()
  res.json(note)
})
app.delete('/api/notes/:id', (req, res) => {
  const before = db.notes.length
  db.notes = db.notes.filter((n) => n.id !== Number(req.params.id))
  if (db.notes.length === before) return res.status(404).json({ error: 'not found' })
  save()
  res.status(204).end()
})

app.get('/api/stats', (req, res) => res.json({
  notes: db.notes.length,
  todos: db.todos.length,
  open: db.todos.filter((t) => !t.done).length,
  done: db.todos.filter((t) => t.done).length,
  requests,
  uptime: Math.round((Date.now() - started) / 1000),
  runtime
}))
app.use((req, res) => res.status(404).json({ error: 'no route', path: req.path }))
app.use((err, req, res, next) => res.status(err.status || 500).json({ error: err.message }))

app.listen(port, () => console.log(`notes + todo on http://localhost:${port}`))
