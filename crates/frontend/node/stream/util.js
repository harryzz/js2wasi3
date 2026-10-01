// From the readable-stream npm package 4.7.0 (MIT, Node.js contributors), adapted by js2wasi3 (T60) as a
// support module for node 24's stream code (scripts/gen-node-streams.mjs); changes are marked "js2wasi3".
'use strict'

const bufferModule = require("node:buffer")
const { format, inspect } = require("node:internal/util/inspect")
const {
  codes: { ERR_INVALID_ARG_TYPE }
} = require("node:internal/errors")
const { kResistStopPropagation, AggregateError, SymbolDispose } = require("node:internal/primordials")
const AbortSignal = require("node:internal/abort").AbortSignal
const AbortController = require("node:internal/abort").AbortController
const Blob = undefined // js2wasi3: no Blob yet
/* eslint-disable indent */
const isBlob =
  typeof Blob !== 'undefined'
    ? function isBlob(b) {
        // eslint-disable-next-line indent
        return b instanceof Blob
      }
    : function isBlob(b) {
        return false
      }
/* eslint-enable indent */

const validateAbortSignal = (signal, name) => {
  if (signal !== undefined && (signal === null || typeof signal !== 'object' || !('aborted' in signal))) {
    throw new ERR_INVALID_ARG_TYPE(name, 'AbortSignal', signal)
  }
}
const validateFunction = (value, name) => {
  if (typeof value !== 'function') {
    throw new ERR_INVALID_ARG_TYPE(name, 'Function', value)
  }
}
module.exports = {
  AggregateError,
  kEmptyObject: Object.freeze({}),
  once(callback) {
    let called = false
    return function (...args) {
      if (called) {
        return
      }
      called = true
      callback.apply(this, args)
    }
  },
  createDeferredPromise: function () {
    let resolve
    let reject

    // eslint-disable-next-line promise/param-names
    const promise = new Promise((res, rej) => {
      resolve = res
      reject = rej
    })
    return {
      promise,
      resolve,
      reject
    }
  },
  promisify(fn) {
    return new Promise((resolve, reject) => {
      fn((err, ...args) => {
        if (err) {
          return reject(err)
        }
        return resolve(...args)
      })
    })
  },
  debuglog() {
    return function () {}
  },
  format,
  inspect,
  types: {
    // js2wasi3: no `AsyncFunction` constructor as a value; node's `util.types.isAsyncFunction`.
    isAsyncFunction(fn) {
      const t = typeof fn === 'function' ? __intrinsic_builtin_tag(fn) : undefined
      return t === 'AsyncFunction' || t === 'AsyncGeneratorFunction'
    },
    isArrayBufferView(arr) {
      return ArrayBuffer.isView(arr)
    }
  },
  isBlob,
  // js2wasi3 (T60): what node 24's streams take from node's other internals (`gen-node-streams.mjs`).
  isArrayBufferView: (v) => ArrayBuffer.isView(v),
  isUint8Array: (v) => v instanceof Uint8Array,
  getOptionValue: () => false,
  isWindows: false,
  assignFunctionName(name, fn) {
    return fn
  },
  deprecate(fn, message) {
    return fn
  },
  addAbortListener:
    function addAbortListener(signal, listener) {
      if (signal === undefined) {
        throw new ERR_INVALID_ARG_TYPE('signal', 'AbortSignal', signal)
      }
      validateAbortSignal(signal, 'signal')
      validateFunction(listener, 'listener')
      let removeEventListener
      if (signal.aborted) {
        queueMicrotask(() => listener())
      } else {
        signal.addEventListener('abort', listener, {
          __proto__: null,
          once: true,
          [kResistStopPropagation]: true
        })
        removeEventListener = () => {
          signal.removeEventListener('abort', listener)
        }
      }
      return {
        __proto__: null,
        [SymbolDispose]() {
          var _removeEventListener
          ;(_removeEventListener = removeEventListener) === null || _removeEventListener === undefined
            ? undefined
            : _removeEventListener()
        }
      }
    },
  AbortSignalAny:
    AbortSignal.any ||
    function AbortSignalAny(signals) {
      // Fast path if there is only one signal.
      if (signals.length === 1) {
        return signals[0]
      }
      const ac = new AbortController()
      const abort = () => ac.abort()
      signals.forEach((signal) => {
        validateAbortSignal(signal, 'signals')
        signal.addEventListener('abort', abort, {
          once: true
        })
      })
      ac.signal.addEventListener(
        'abort',
        () => {
          signals.forEach((signal) => signal.removeEventListener('abort', abort))
        },
        {
          once: true
        }
      )
      return ac.signal
    }
}
module.exports.promisify.custom = Symbol.for('nodejs.util.promisify.custom')
