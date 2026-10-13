//! Backend: IR → core Wasm bytes (WasmGC types, exception tag, linear memory for the ABI only).
//!
//! v0 (T06) handles the numbers-only IR from T05: `f64`/`i32`/`bool` locals and globals,
//! structured control flow, direct calls and exports. T08 adds `jsval` (see [`runtime`]).
//!
//! # Mapping
//! - `Bool` is an `i32` holding 0 or 1.
//! - `Block L { .. }` → `block`. `Loop L { .. }` → `block $brk (loop $cont ..)`: `Break L` branches
//!   to `$brk`, `Continue L` to `$cont`. `if` also opens a label level, which depth computation accounts for.
//! - `Throw e` → `throw $js`. `Try { body, catch_local, catch_body }` →
//!   `block (block (result jsval) (try_table (catch $js 0) body) (br 1)) (local.set catch_local) catch_body`:
//!   no `catch_all`, so Wasm traps are never caught (design D2).
//! - (T14) `Closure` → `ref.func` of its code, the environment as an `array.new_fixed $vals`, then
//!   `$new_function` (see [`func`]); a shared closure (a top-level function) is made once into its
//!   own global (after the module's globals, before the strings). `CallRef` → the arguments in a
//!   `$vals` array and `$call_value` (`call_ref $code`). Cells are `$cell_f64`/`$cell` structs; the
//!   IR carries them as `jsval`, so cell and environment accesses start with a `ref.cast`. Functions
//!   with the `$code` signature get the runtime's `$code` type, and every `ref.func` target is in a
//!   declarative element segment.
//! - (T15) `Array` → `array.new_fixed $vals` (holes: `ref.null`) and `$new_array`; iteration, spread and
//!   destructuring intrinsics call the helpers in [`iter`], [`array`] and [`object`]; `console.log`
//!   formatting is [`inspect`].
//! - A function with a result ends in `unreachable`: the lowering guarantees every path returns,
//!   but Wasm validation can't see through an `if/else` whose two arms both `return`.
//! - Runtime helpers are emitted after the user functions, and only when used:
//!   - `$js_rem(x, y)`: JS `%` on numbers, i.e. an exact `fmod` (a port of musl's bit-level algorithm).
//!     The shortcut `x - trunc(x/y)*y` is inexact for large quotients.
//!   - `$to_int32(x)`: JS `ToInt32`. `i64.trunc_sat_f64_s` + wrap when |x| < 2^63, otherwise
//!     `fmod(x, 2^32)` first. NaN and ±∞ give 0. A conversion is inline when |x| < 2^31
//!     (`i32.trunc_f64_s`, through each function's scratch `f64` local) and calls it otherwise (T27).
//!
//!   - JS value helpers (`$box_number`, `$to_number`, `$strict_eq`, …): see [`runtime`].
//! - A module that uses `jsval` starts its type section with the runtime `rec` group and its
//!   global section with the four singletons, so user types and globals are offset by those.
//!
//! Unsupported IR (placeholders) is reported with the IR kind, the function and
//! its source position before any bytes are emitted.

mod abi;
mod array;
mod asyncfn;
mod asyncgen;
mod bigint;
mod func;
mod generator;
mod host;
mod inspect;
mod iter;
mod mapset;
mod numstr;
mod object;
mod pow;
mod promise;
mod props;
mod regex;
mod runtime;
mod timers;
mod typed;
mod wit;
mod wrapper;

use std::collections::HashMap;

use anyhow::{Context, bail};
use js2wasi3_ir::{
    self as ir, AbiType, BinOp, Const, Expr, Intrinsic, LabelId, Stmt, Type, UnOp, visit_exprs,
};
use runtime::JsTest as _;
use wasm_encoder::{
    BlockType, Catch, CodeSection, ConstExpr, DataCountSection, DataSection, ElementSection,
    Elements, ExportKind, ExportSection, Function, FunctionSection, GlobalSection, GlobalType,
    ImportSection, IndirectNameMap, InstructionSink, MemorySection, MemoryType, NameMap,
    NameSection, StartSection, TableSection, TableType, TagKind, TagSection, TagType, TypeSection,
    ValType,
};

/// Emit a core Wasm module for `module`.
pub fn emit(module: &ir::Module) -> anyhow::Result<Vec<u8>> {
    if let Err(errs) = ir::validate(module) {
        bail!(
            "internal error: invalid IR from the lowering:\n  {}",
            errs.join("\n  ")
        );
    }
    check_supported(module)?;
    if let Some(f) = module.string_to_number {
        let f = module.function(f);
        if f.params != 1 || f.locals[0].ty != Type::JsVal || f.result != Some(Type::F64) {
            bail!("internal error: the prelude's StringToNumber must be (jsval) -> f64");
        }
    }
    if let Some(f) = module.date_inspect {
        let f = module.function(f);
        if f.params != 1 || f.locals[0].ty != Type::JsVal || f.result != Some(Type::JsVal) {
            bail!("internal error: the prelude's $$dateInspect must be (jsval) -> jsval");
        }
    }
    if let Some(f) = module.inspect_custom {
        let f = module.function(f);
        if f.params != 2
            || f.locals[..2].iter().any(|l| l.ty != Type::JsVal)
            || f.result != Some(Type::JsVal)
        {
            bail!("internal error: the prelude's $$inspectCustom must be (jsval, jsval) -> jsval");
        }
    }
    if let Some(f) = module.process_at_exit {
        let f = module.function(f);
        if f.params != 1
            || f.locals[0].ty != Type::JsVal
            || f.result.is_some_and(|t| t != Type::JsVal)
        {
            bail!("internal error: the prelude's $$processAtExit must be (jsval) -> jsval?");
        }
    }

    // A command (`wasi:cli/run`) always has the JS runtime and memory (for output buffers), a
    // library with `init` the runtime (its globals come after the runtime's, T20).
    let command = module.main;
    let js = uses_jsval(module) || command.is_some() || module.init.is_some();
    // Exports with a string at the boundary go through an ABI adapter (and need memory).
    let adapted: Vec<&ir::Export> = module
        .exports
        .iter()
        .filter(|x| {
            x.sig.params.iter().any(|p| *p != AbiType::F64)
                || x.sig.result.is_some_and(|r| r != AbiType::F64)
                || x.sig.throws
                || x.sig.is_async
                || module.init.is_some()
        })
        .collect();
    // Linear memory: output buffers, strings and `result`s at the boundary.
    // A user world's imports (T20 slice 5) pass their arguments in memory.
    let abi = adapted.iter().any(|x| abi::needs_memory(&x.sig))
        || command.is_some()
        || !module.imports.is_empty();
    let mut types = TypeSection::new();
    if js {
        runtime::types(&mut types, module.bigint, module.rab);
    }
    let n = module.functions.len() as u32;
    // Imported functions come first in the function index space.
    let (mut random, mut clock, mut env, mut exits) = (false, false, [false; 3], false);
    for f in &module.functions {
        visit_exprs(&f.body, &mut |x| {
            if let Expr::Intrinsic { op, .. } = x {
                random |= *op == Intrinsic::RandomSeed;
                clock |= *op == Intrinsic::DateNow;
                if let Intrinsic::Environment(k) = op {
                    env[*k as usize] = true;
                }
                exits |= *op == Intrinsic::ProcessExit;
            }
        });
    }
    // `process` (T55): `process.exit`'s tag and the ending state (a host global).
    let exits = command.is_some() && (exits || module.process_at_exit.is_some());
    let import_list = if command.is_some() {
        host::import_list(random, clock, env, !abi::payloads(module).is_empty())
    } else {
        Vec::new()
    };
    // A library's async exports (T20): each imports its `task.return` and exports a lift and a
    // callback (after the adapters).
    let lifted: Vec<&ir::Export> = adapted.iter().copied().filter(|x| x.sig.is_async).collect();
    // A library's timers run while an async export waits (T20).
    let lib_timers = !lifted.is_empty() && uses_timers(module);
    // `async func` imports (T20 slice 5b) wait in the async export's waitable set.
    let async_imports = module.imports.iter().any(|x| x.sig.is_async);
    // The `future`s at the boundary (T20 slice 6a): their intrinsics come after the user
    // world's imports, [`abi::PAYLOAD_OPS`] per future type.
    let payloads = abi::payloads(module);
    let import_list = if lib_timers {
        host::LIBRARY_TIMER_IMPORTS.to_vec()
    } else if (async_imports || !payloads.is_empty()) && !lifted.is_empty() {
        host::LIBRARY_WAIT_IMPORTS.to_vec()
    } else {
        import_list
    };
    // A user world's imports come after the host's (T20 slice 5).
    let user_imports = import_list.len() as u32 + lifted.len() as u32;
    let payload_base = user_imports + module.imports.len() as u32;
    // A command with non-blocking imports (T66: `wasi:sockets`) also imports each boundary
    // type's cancel-read, after the other intrinsics.
    let command_waits = command.is_some() && module.imports.iter().any(|x| !x.sig.blocking);
    let cancel_base = payload_base + abi::PAYLOAD_OPS * payloads.len() as u32;
    let cancels = if command_waits {
        payloads.len() as u32
    } else {
        0
    };
    let func_base = cancel_base + cancels;
    // Globals: runtime, ABI bump pointer, host globals, then the module's own.
    let runtime_globals = runtime::runtime_globals(
        module.bigint,
        module.suppressed,
        module.stacks,
        module.big_typed,
    );
    let host_base = runtime_globals + abi as u32;
    // A command with non-blocking imports (T66: `wasi:sockets`) keeps a library's pending
    // imports and streams ([`abi::LIB_GLOBALS`]) after its host globals.
    let command_host = host::GLOBALS + exits as u32 * host::EXIT_GLOBALS;
    let lib_base = if command.is_some() {
        host_base + command_host
    } else {
        host_base
    };
    let host_globals = if command.is_some() {
        command_host + command_waits as u32 * abi::LIB_GLOBALS
    } else if module.init.is_some() || module.exports.iter().any(|x| x.sig.is_async) {
        abi::LIB_GLOBALS
    } else {
        0
    };
    // Top-level function objects (shared closures): one global each, after the module's globals.
    let mut shared = Vec::new();
    for f in &module.functions {
        visit_exprs(&f.body, &mut |x| {
            if let Expr::Closure(c) = x
                && c.shared
                && !shared.contains(&c.code)
            {
                shared.push(c.code);
            }
        });
    }
    let type_base = if js {
        runtime::TYPES + module.bigint as u32
    } else {
        0
    };
    // T110: each key's distinct slots in the layouts that have it, in first-appearance order.
    let mut layout_slots: HashMap<Vec<u16>, Vec<u32>> = HashMap::new();
    for keys in &module.layouts {
        for (i, k) in keys.iter().enumerate() {
            let v = layout_slots.entry(k.encode_utf16().collect()).or_default();
            if !v.contains(&(i as u32)) {
                v.push(i as u32);
            }
        }
    }
    let mut e = Emitter {
        scratch_f64: 0,
        scratch_js: 0,
        scratch_obj: 0,
        scratch_val: 0,
        scratch_i32: 0,
        scratch_vals: 0,
        scratch_r: 0,
        layout_slots,
        m: module,
        exits,
        imports: host::Imports {
            base: lifted.len() as u32,
            list: import_list.clone(),
        },
        types,
        next_type: type_base,
        global_base: if js { host_base + host_globals } else { 0 },
        bump: runtime_globals,
        user_imports,
        payload_base,
        cancel_base: command_waits.then_some(cancel_base),
        payloads: payloads.clone(),
        host_base,
        lib_base,
        command_waits,
        func_base,
        type_ids: HashMap::new(),
        helper_base: func_base + n + adapted.len() as u32 + 3 * lifted.len() as u32,
        helpers: Vec::new(),
        regexes: module.regexes.clone(),
        regex_vm: module.regex_vm,
        strings: Vec::new(),
        string_ids: HashMap::new(),
        tables: false,
        shared,
        declared: Vec::new(),
        code_table: Vec::new(),
        code_slots: HashMap::new(),
        ic_sites: 0,
        set_sites: 0,
    };
    if js {
        // Functions with the `$code` signature use the runtime's type (`call_ref` needs it).
        let v = runtime::jsval();
        e.type_ids.insert((vec![v, v, v], vec![v]), runtime::T_CODE);
        // Generators' resume functions (T18): `call_ref` from `next`.
        e.type_ids
            .insert((vec![v, ValType::I32, v], vec![v]), runtime::T_RESUME);
    }

    let mut imports = ImportSection::new();
    for x in &lifted {
        let ty = e.type_id(abi::task_return_params(&x.sig), vec![]);
        // An interface's function (`ns:pkg/iface#f`, T20 slice 5) returns through its interface.
        let (module, name) = match x.name.split_once('#') {
            Some((iface, f)) => (format!("[export]{iface}"), f),
            None => ("[export]$root".to_string(), x.name.as_str()),
        };
        imports.import(
            &module,
            &format!("[task-return]{name}"),
            wasm_encoder::EntityType::Function(ty),
        );
    }
    host::imports(&mut imports, &import_list, &mut |p, r| e.type_id(p, r));
    for x in &module.imports {
        let (params, results) = abi::import_type(&x.sig);
        let ty = e.type_id(params, results);
        let name = if x.sig.is_async {
            format!("[async-lower]{}", x.name)
        } else {
            x.name.clone()
        };
        imports.import(&x.module, &name, wasm_encoder::EntityType::Function(ty));
    }
    for p in &payloads {
        for op in abi::PayloadOp::ALL {
            let (params, results) = op.signature(p.stream);
            let ty = e.type_id(params, results);
            imports.import(
                &p.module,
                &op.name(p),
                wasm_encoder::EntityType::Function(ty),
            );
        }
    }
    if command_waits {
        let ty = e.type_id(vec![ValType::I32], vec![ValType::I32]);
        for p in &payloads {
            let name = abi::cancel_read_name(p);
            imports.import(&p.module, &name, wasm_encoder::EntityType::Function(ty));
        }
    }
    // The exception tag (`runtime::TAG`) comes with the JS runtime.
    let mut tags = TagSection::new();
    if js {
        tags.tag(TagType {
            kind: TagKind::Exception,
            func_type_idx: e.type_id(vec![runtime::jsval()], vec![]),
        });
    }
    if exits {
        // `host::EXIT_TAG` (T55).
        tags.tag(TagType {
            kind: TagKind::Exception,
            func_type_idx: e.type_id(vec![], vec![]),
        });
    }
    let mut funcs = FunctionSection::new();
    let mut code = CodeSection::new();
    for f in &module.functions {
        let params: Vec<ValType> = f.locals[..f.params as usize]
            .iter()
            .map(|l| val_type(l.ty))
            .collect();
        let results: Vec<ValType> = f.result.map(val_type).into_iter().collect();
        funcs.function(e.type_id(params, results));
        code.function(
            &e.function(f)
                .with_context(|| format!("in function {}", f.name))?,
        );
    }
    let mut adapter_names = Vec::new();
    for x in &adapted {
        let params = abi::flat_params(&x.sig);
        // An async export's adapter starts the call (T20).
        let (results, mode) = if x.sig.is_async {
            (vec![], abi::Mode::Start)
        } else {
            (abi::flat_result(&x.sig), abi::Mode::Sync)
        };
        funcs.function(e.type_id(params, results));
        let bump = e.bump;
        code.function(&abi::adapter(
            &mut e,
            bump,
            func_base + x.func.0,
            &x.sig,
            mode,
        ));
        adapter_names.push(format!("{}$abi", x.name));
    }
    let adapter_index = |x: &ir::Export| {
        func_base + n + adapted.iter().position(|a| std::ptr::eq(*a, x)).unwrap() as u32
    };
    // Per async export: the adapter that finishes the call, the lift and its callback.
    for (j, x) in lifted.iter().enumerate() {
        let finish = func_base + n + adapted.len() as u32 + 3 * j as u32;
        let task_return = j as u32;
        funcs.function(e.type_id(vec![], vec![]));
        let bump = e.bump;
        let mode = abi::Mode::Finish(task_return);
        code.function(&abi::adapter(&mut e, bump, 0, &x.sig, mode));
        let params = abi::flat_params(&x.sig);
        funcs.function(e.type_id(params, vec![ValType::I32]));
        code.function(&abi::async_lift(&mut e, adapter_index(x), finish, &x.sig));
        funcs.function(e.type_id(vec![ValType::I32; 3], vec![ValType::I32]));
        code.function(&abi::async_callback(&mut e, finish));
    }
    let realloc =
        abi.then(|| e.helper_index(Helper::Js(runtime::Helper::Abi(abi::Helper::Realloc))));
    let run = command.map(|_| {
        (
            e.helper_index(Helper::Js(runtime::Helper::Host(host::Helper::Run))),
            e.helper_index(Helper::Js(runtime::Helper::Host(host::Helper::Callback))),
        )
    });
    // Helpers can request other helpers, so iterate until the list stops growing.
    let mut i = 0;
    while i < e.helpers.len() {
        let h = e.helpers[i];
        let (params, results) = h.signature();
        funcs.function(e.type_id(params, results));
        code.function(&e.helper(h));
        i += 1;
    }
    // The constants' initializers (T57), after the helpers; the last one is the start function.
    let const_data = e.const_inits(&mut funcs, &mut code);
    let init_base = e.helper_base + e.helpers.len() as u32;
    let start = (!const_data.1.is_empty()).then(|| init_base + const_data.1.len() as u32 - 1);

    let mut globals = GlobalSection::new();
    if js {
        runtime::globals(
            &mut globals,
            module.bigint,
            module.suppressed,
            module.stacks,
            module.big_typed,
        );
    }
    if abi {
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &ConstExpr::i32_const(abi::HEAP_BASE),
        );
    }
    for _ in 0..if command.is_some() { command_host } else { 0 } {
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &ConstExpr::i32_const(0),
        );
    }
    if (command.is_none() && host_globals == abi::LIB_GLOBALS) || command_waits {
        // `abi::LIB_GLOBALS`: `init`'s state and error, an async call's state (T20).
        for ty in abi::lib_global_types() {
            let init = if ty == ValType::I32 {
                ConstExpr::i32_const(0)
            } else {
                ConstExpr::ref_null(wasm_encoder::HeapType::Abstract {
                    shared: false,
                    ty: wasm_encoder::AbstractHeapType::Eq,
                })
            };
            globals.global(
                GlobalType {
                    val_type: ty,
                    mutable: true,
                    shared: false,
                },
                &init,
            );
        }
    }
    for g in &module.globals {
        let init = match (g.ty, g.init) {
            (Type::JsVal, Const::F64(v)) => runtime::const_number(v),
            (Type::JsVal, Const::Bool(v)) => {
                ConstExpr::global_get(if v { runtime::G_TRUE } else { runtime::G_FALSE })
            }
            (_, Const::Undefined) => ConstExpr::global_get(runtime::G_UNDEFINED),
            (_, Const::Null) => ConstExpr::global_get(runtime::G_NULL),
            (_, Const::F64(v)) => ConstExpr::f64_const(v.into()),
            (_, Const::I32(v)) => ConstExpr::i32_const(v),
            (_, Const::Bool(v)) => ConstExpr::i32_const(v as i32),
        };
        globals.global(
            GlobalType {
                val_type: val_type(g.ty),
                mutable: g.mutable,
                shared: false,
            },
            &init,
        );
    }

    for _ in &e.shared {
        globals.global(
            GlobalType {
                val_type: runtime::jsval(),
                mutable: true,
                shared: false,
            },
            &ConstExpr::ref_null(wasm_encoder::HeapType::Abstract {
                shared: false,
                ty: wasm_encoder::AbstractHeapType::Eq,
            }),
        );
    }
    // T109: each layout's shared hash index (`object::layout_index`), immutable.
    for keys in &module.layouts {
        let units: Vec<Vec<u16>> = keys.iter().map(|k| k.encode_utf16().collect()).collect();
        let idx = object::layout_index(&units);
        let mut init: Vec<wasm_encoder::Instruction> =
            idx.iter().map(|&e| wasm_encoder::Instruction::I32Const(e)).collect();
        init.push(wasm_encoder::Instruction::ArrayNewFixed {
            array_type_index: runtime::T_I32S,
            array_size: idx.len() as u32,
        });
        globals.global(
            GlobalType {
                val_type: runtime::rt(false, runtime::T_I32S),
                mutable: false,
                shared: false,
            },
            &ConstExpr::extended(init),
        );
    }
    // String and BigInt constants (T57): mutable globals holding a placeholder (the first one or
    // two globals here), filled by the start function (see `const_inits`).
    if !e.strings.is_empty() {
        runtime::const_placeholders(&mut globals, module.bigint);
    }
    for c in &e.strings {
        match c {
            ConstGlobal::Str(_) => {
                runtime::string_global(&mut globals, e.const_base() - 1 - module.bigint as u32)
            }
            ConstGlobal::BigInt(_) => bigint::const_global(&mut globals, e.const_base() - 1),
        }
    }

    let mut exports = ExportSection::new();
    let mut next_adapter = func_base + n;
    for x in &module.exports {
        if let Some(j) = lifted.iter().position(|a| std::ptr::eq(*a, x)) {
            let lift = func_base + n + adapted.len() as u32 + 3 * j as u32 + 1;
            exports.export(&format!("[async-lift]{}", x.name), ExportKind::Func, lift);
            exports.export(
                &format!("[callback][async-lift]{}", x.name),
                ExportKind::Func,
                lift + 1,
            );
            next_adapter += 1;
            continue;
        }
        let index = if adapted.iter().any(|a| std::ptr::eq(*a, x)) {
            next_adapter += 1;
            next_adapter - 1
        } else {
            func_base + x.func.0
        };
        exports.export(&x.name, ExportKind::Func, index);
    }
    if let Some((run, callback)) = run {
        exports.export("[async-lift]wasi:cli/run@0.3.1#run", ExportKind::Func, run);
        exports.export(
            "[callback][async-lift]wasi:cli/run@0.3.1#run",
            ExportKind::Func,
            callback,
        );
    }
    let mut memory = MemorySection::new();
    if let Some(realloc) = realloc {
        memory.memory(MemoryType {
            minimum: 1,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        exports.export("memory", ExportKind::Memory, 0);
        exports.export("cabi_realloc", ExportKind::Func, realloc);
    }

    let mut names = NameSection::new();
    let mut fnames = NameMap::new();
    let mut lnames = IndirectNameMap::new();
    for (j, x) in lifted.iter().enumerate() {
        fnames.append(j as u32, &format!("[task-return]{}", x.name));
    }
    for (i, imp) in import_list.iter().enumerate() {
        let (module, field) = imp.names();
        fnames.append(lifted.len() as u32 + i as u32, &format!("{module}#{field}"));
    }
    for (i, x) in module.imports.iter().enumerate() {
        fnames.append(user_imports + i as u32, &format!("{}#{}", x.module, x.name));
    }
    for (i, p) in payloads.iter().enumerate() {
        for (k, op) in abi::PayloadOp::ALL.iter().enumerate() {
            let at = payload_base + (i as u32) * abi::PAYLOAD_OPS + k as u32;
            fnames.append(at, &format!("{}#{}", p.module, op.name(p)));
        }
    }
    if command_waits {
        for (i, p) in payloads.iter().enumerate() {
            let at = cancel_base + i as u32;
            fnames.append(at, &format!("{}#{}", p.module, abi::cancel_read_name(p)));
        }
    }
    for (i, f) in module.functions.iter().enumerate() {
        fnames.append(func_base + i as u32, &f.name);
        let mut locals = NameMap::new();
        for (j, l) in f.locals.iter().enumerate() {
            locals.append(j as u32, &l.name);
        }
        lnames.append(func_base + i as u32, &locals);
    }
    for (k, name) in adapter_names.iter().enumerate() {
        fnames.append(func_base + n + k as u32, name);
    }
    for (j, x) in lifted.iter().enumerate() {
        let lift = func_base + n + adapted.len() as u32 + 3 * j as u32 + 1;
        fnames.append(lift - 1, &format!("{}$finish", x.name));
        fnames.append(lift, &format!("{}$lift", x.name));
        fnames.append(lift + 1, &format!("{}$callback", x.name));
    }
    for (k, h) in e.helpers.iter().enumerate() {
        fnames.append(e.helper_base + k as u32, &h.name());
    }
    for (k, name) in const_data.1.iter().enumerate() {
        fnames.append(init_base + k as u32, name);
    }
    names.functions(&fnames);
    names.locals(&lnames);

    let mut out = wasm_encoder::Module::new();
    out.section(&e.types);
    if !imports.is_empty() {
        out.section(&imports);
    }
    out.section(&funcs);
    // The code table (T89): every function a `$function` holds, called with `call_indirect`.
    if !e.code_table.is_empty() {
        let mut tables = TableSection::new();
        let n = e.code_table.len() as u64;
        tables.table(TableType {
            element_type: wasm_encoder::RefType::FUNCREF,
            table64: false,
            minimum: n,
            maximum: Some(n),
            shared: false,
        });
        out.section(&tables);
    }
    if abi {
        out.section(&memory);
    }
    if js {
        out.section(&tags);
    }
    if !globals.is_empty() {
        out.section(&globals);
    }
    out.section(&exports);
    if let Some(function_index) = start {
        out.section(&StartSection { function_index });
    }
    if !e.declared.is_empty() || !e.code_table.is_empty() {
        let mut elems = ElementSection::new();
        if !e.code_table.is_empty() {
            elems.active(
                Some(0),
                &ConstExpr::i32_const(0),
                Elements::Functions(e.code_table.clone().into()),
            );
        }
        if !e.declared.is_empty() {
            elems.declared(Elements::Functions(e.declared.clone().into()));
        }
        out.section(&elems);
    }
    // Data segments: the number → string tables (0, when used), then the constants' contents.
    let segments = e.tables as u32 + !const_data.0.is_empty() as u32;
    if segments > 0 {
        out.section(&DataCountSection { count: segments });
    }
    out.section(&code);
    if segments > 0 {
        let mut data = DataSection::new();
        if e.tables {
            data.passive(numstr::table_bytes().iter().copied());
        }
        if !const_data.0.is_empty() {
            data.passive(const_data.0.iter().copied());
        }
        out.section(&data);
    }
    out.section(&names);
    Ok(out.finish())
}

/// Does the module use timers, immediates or `process.nextTick` (T19, T25)?
fn uses_timers(module: &ir::Module) -> bool {
    ["$timers", "$immediates", "$ticks"]
        .iter()
        .any(|n| module.names.contains(*n))
}

/// The cell struct for a variable of type `t`.
fn cell_type(t: Type) -> u32 {
    match t {
        Type::F64 => runtime::T_CELL_F64,
        _ => runtime::T_CELL,
    }
}

/// T75: an operand of `===` that is a constant with no effects.
enum EqOperand<'a> {
    Oddball(u32),
    Str(&'a [u16]),
    Num(f64),
}

fn eq_const(e: &Expr) -> Option<EqOperand<'_>> {
    Some(match e {
        Expr::Const(Const::Undefined) => EqOperand::Oddball(runtime::G_UNDEFINED),
        Expr::Const(Const::Null) => EqOperand::Oddball(runtime::G_NULL),
        Expr::Str(units) => EqOperand::Str(units),
        Expr::Convert {
            from: Type::Bool,
            to: Type::JsVal,
            arg,
        } => match **arg {
            Expr::Const(Const::Bool(b)) => {
                EqOperand::Oddball(if b { runtime::G_TRUE } else { runtime::G_FALSE })
            }
            _ => return None,
        },
        Expr::Convert {
            from: Type::F64,
            to: Type::JsVal,
            arg,
        } => match **arg {
            Expr::Const(Const::F64(v)) => EqOperand::Num(v),
            _ => return None,
        },
        _ => return None,
    })
}

fn val_type(t: Type) -> ValType {
    match t {
        Type::F64 => ValType::F64,
        Type::I32 | Type::Bool => ValType::I32,
        Type::JsVal => runtime::jsval(),
        other => unreachable!("no Wasm type for {other}"),
    }
}

/// Does the module need the JS value runtime?
fn uses_jsval(m: &ir::Module) -> bool {
    fn expr(e: &Expr) -> bool {
        match e {
            Expr::Const(c) => c.ty() == Type::JsVal,
            Expr::Local(_) | Expr::Global(_) => false,
            Expr::Convert { from, to, arg } => {
                *from == Type::JsVal || *to == Type::JsVal || expr(arg)
            }
            Expr::Binary { ty, lhs, rhs, .. } => *ty == Type::JsVal || expr(lhs) || expr(rhs),
            Expr::Cond {
                cond,
                then,
                els,
                ty,
            } => *ty == Type::JsVal || expr(cond) || expr(then) || expr(els),
            Expr::TypeofIs { .. } | Expr::Suspend { .. } | Expr::Str(_) => true,
            Expr::Intrinsic { .. } => true,
            Expr::Tee(_, a) | Expr::Unary { arg: a, .. } => expr(a),
            Expr::Call { args, .. } => args.iter().any(expr),
            Expr::Seq { body, value } => stmts(body) || expr(value),
            Expr::Closure(_) | Expr::CallRef { .. } | Expr::Array(_) | Expr::New { .. } => true,
        }
    }
    fn stmts(list: &[Stmt]) -> bool {
        list.iter().any(|s| match s {
            Stmt::Throw(_) | Stmt::Try { .. } => true, // the tag's type is `jsval`
            Stmt::Expr(e) | Stmt::Set(_, e) | Stmt::SetGlobal(_, e) => expr(e),
            Stmt::Return(e) => e.as_ref().is_some_and(expr),
            Stmt::Block { body, .. } | Stmt::Loop { body, .. } => stmts(body),
            Stmt::If { cond, then, els } => expr(cond) || stmts(then) || stmts(els),
            Stmt::Break(_) | Stmt::Continue(_) => false,
        })
    }
    m.globals.iter().any(|g| g.ty == Type::JsVal)
        || m.functions.iter().any(|f| {
            f.result == Some(Type::JsVal)
                || f.locals.iter().any(|l| l.ty == Type::JsVal)
                || stmts(&f.body)
        })
}

// ---- unsupported IR ------------------------------------------------------------------------

fn check_supported(m: &ir::Module) -> anyhow::Result<()> {
    let mut errs = Vec::new();
    for f in &m.functions {
        let at = f
            .pos
            .map(|p| format!(" at {}:{}", p.line, p.col))
            .unwrap_or_default();
        let mut report = |kind: &str, task: &str| {
            errs.push(format!(
                "unsupported IR `{kind}` in function {}{at} (planned: {task})",
                f.name
            ))
        };
        // Generators are rewritten by `ir::suspend::lower_generators` (run by the lowering, T18).
        if f.flags.is_async {
            report("async function before `ir::suspend`", "T19");
        }
        if f.flags.is_generator {
            report("generator function before `ir::suspend`", "T18");
        }
        let mut kinds = Vec::new();
        unsupported_stmts(&f.body, &mut kinds);
        kinds.dedup();
        for (kind, task) in kinds {
            report(kind, task);
        }
    }
    if errs.is_empty() {
        Ok(())
    } else {
        bail!("{}", errs.join("\n"))
    }
}

fn unsupported_stmts(list: &[Stmt], out: &mut Vec<(&'static str, &'static str)>) {
    for s in list {
        match s {
            Stmt::Try {
                body, catch_body, ..
            } => {
                unsupported_stmts(body, out);
                unsupported_stmts(catch_body, out);
            }
            Stmt::Expr(e)
            | Stmt::Set(_, e)
            | Stmt::SetGlobal(_, e)
            | Stmt::Return(Some(e))
            | Stmt::Throw(e) => unsupported_expr(e, out),
            Stmt::Block { body, .. } | Stmt::Loop { body, .. } => unsupported_stmts(body, out),
            Stmt::If { cond, then, els } => {
                unsupported_expr(cond, out);
                unsupported_stmts(then, out);
                unsupported_stmts(els, out);
            }
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Return(None) => {}
        }
    }
}

fn unsupported_expr(e: &Expr, out: &mut Vec<(&'static str, &'static str)>) {
    match e {
        Expr::Suspend { arg, .. } => {
            out.push((
                "Suspend",
                "T19; a `yield` must go through `ir::suspend` first",
            ));
            unsupported_expr(arg, out);
        }
        Expr::Const(_) | Expr::Local(_) | Expr::Global(_) => {}
        Expr::Tee(_, a)
        | Expr::Unary { arg: a, .. }
        | Expr::Convert { arg: a, .. }
        | Expr::TypeofIs { arg: a, .. } => unsupported_expr(a, out),
        Expr::Str(_) => {}
        Expr::Intrinsic { args, .. } => args.iter().for_each(|a| unsupported_expr(a, out)),
        Expr::Seq { body, value } => {
            unsupported_stmts(body, out);
            unsupported_expr(value, out);
        }
        Expr::Binary { lhs, rhs, .. } => {
            unsupported_expr(lhs, out);
            unsupported_expr(rhs, out);
        }
        Expr::Cond {
            cond, then, els, ..
        } => {
            unsupported_expr(cond, out);
            unsupported_expr(then, out);
            unsupported_expr(els, out);
        }
        Expr::Call { args, .. } => args.iter().for_each(|a| unsupported_expr(a, out)),
        Expr::Closure(c) => c.env.iter().for_each(|a| unsupported_expr(a, out)),
        Expr::Array(elems) => elems
            .iter()
            .flatten()
            .for_each(|a| unsupported_expr(a, out)),
        Expr::CallRef {
            callee, this, args, ..
        } => {
            unsupported_expr(callee, out);
            unsupported_expr(this, out);
            args.iter().for_each(|a| unsupported_expr(a, out));
        }
        Expr::New {
            callee,
            new_target,
            args,
            ..
        } => {
            unsupported_expr(callee, out);
            new_target.iter().for_each(|a| unsupported_expr(a, out));
            args.iter().for_each(|a| unsupported_expr(a, out));
        }
    }
}

// ---- emitter -------------------------------------------------------------------------------

/// Constants per initializer function (T57).
const CONST_CHUNK: usize = 1024;

/// A constant global after the module's own: a string, or a BigInt's words (T40).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum ConstGlobal {
    Str(Vec<u16>),
    BigInt(Vec<i32>),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Helper {
    JsRem,
    JsPow,
    ToInt32,
    Js(runtime::Helper),
    /// T114: `re_exec(prog, s, from, mode, caps) -> i32`: the regex interpreter (`regex.rs`).
    ReExec,
    /// T114: `re_class_wide(prog, cls, c) -> i32`: a unit above 127 in a program's class.
    ReClassWide,
    /// T114: `re_grow(stack, need) -> stack`: the regex backtrack stack, grown.
    ReGrow,
}

impl Helper {
    fn name(self) -> std::borrow::Cow<'static, str> {
        match self {
            Helper::JsRem => "js_rem".into(),
            Helper::JsPow => "js_pow".into(),
            Helper::ToInt32 => "to_int32".into(),
            Helper::Js(h) => h.name().into(),
            Helper::ReExec => "re_exec".into(),
            Helper::ReClassWide => "re_class_wide".into(),
            Helper::ReGrow => "re_grow".into(),
        }
    }

    fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        match self {
            Helper::JsRem | Helper::JsPow => (vec![ValType::F64, ValType::F64], vec![ValType::F64]),
            Helper::ToInt32 => (vec![ValType::F64], vec![ValType::I32]),
            Helper::Js(h) => h.signature(),
            Helper::ReExec => (
                vec![
                    runtime::rt(false, runtime::T_STRING),
                    runtime::rt(false, runtime::T_STRING),
                    ValType::I32,
                    ValType::I32,
                    runtime::rt(false, runtime::T_I32S),
                ],
                vec![ValType::I32],
            ),
            Helper::ReClassWide => (
                vec![runtime::rt(false, runtime::T_STRING), ValType::I32, ValType::I32],
                vec![ValType::I32],
            ),
            Helper::ReGrow => (
                vec![runtime::rt(true, runtime::T_I32S), ValType::I32],
                vec![runtime::rt(false, runtime::T_I32S)],
            ),
        }
    }
}

/// One entry per open Wasm label, innermost last.
#[derive(Clone, Copy, PartialEq)]
enum Frame {
    Break(LabelId),
    Continue(LabelId),
    /// `if` / typed `if` from `Cond`: not a branch target in the IR.
    Anon,
}

/// T112: count a T110 inline site's miss (`build --count`): `o` is null when the receiver wasn't an `$object`
/// (it's reset before the cast), else one of its subtypes or a plain object whose slots didn't match.
fn count_miss(s: &mut wasm_encoder::InstructionSink, on: bool, o: u32, site: &str) {
    if !on {
        return;
    }
    s.local_get(o).ref_is_null();
    s.if_(BlockType::Empty);
    runtime::count(s, on, &format!("{site}.miss_not_object"));
    s.else_();
    runtime::is_object_subtype(s, o);
    s.if_(BlockType::Empty);
    runtime::count(s, on, &format!("{site}.miss_subtype"));
    s.else_();
    runtime::count(s, on, &format!("{site}.miss_plain"));
    s.end();
    s.end();
}

/// T110: the most slots a constant-key read tries inline (a key at more distinct slots calls `get_named`).
const LAYOUT_GUESSES: usize = 3;

struct Emitter<'m> {
    m: &'m ir::Module,
    /// The command uses `process.exit` or `process`'s at-exit hook (T55).
    exits: bool,
    /// The current function's scratch `f64` local (the inline ToInt32 fast path).
    scratch_f64: u32,
    /// A `jsval` local for inline sequences (T75).
    scratch_js: u32,
    /// T110: a `(ref null $object)` and a second `jsval` local for the inline layout reads.
    scratch_obj: u32,
    scratch_val: u32,
    /// T115: an `i32`, a `(ref null $vals)` and a third `jsval` local for the inline element reads.
    scratch_i32: u32,
    scratch_vals: u32,
    scratch_r: u32,
    /// T110: a key's distinct slots in the module's layouts.
    layout_slots: HashMap<Vec<u16>, Vec<u32>>,
    imports: host::Imports,
    types: TypeSection,
    /// Runtime globals that come before the module's own (see `global_base`).
    /// Index of the next function type.
    next_type: u32,
    global_base: u32,
    /// Global index of the ABI bump pointer (when there are string exports).
    bump: u32,
    /// Function index of the user world's first import (T20 slice 5).
    user_imports: u32,
    /// The intrinsics of the boundary's futures (T20 slice 6a).
    payload_base: u32,
    /// The cancel-read imports, one per boundary type (T66: a command waiting for imports).
    cancel_base: Option<u32>,
    payloads: Vec<ir::Payload>,
    /// Global index of the first host global (commands).
    host_base: u32,
    /// Global index of the first of [`abi::LIB_GLOBALS`] (a library's, or a command's after its
    /// host globals, T66).
    lib_base: u32,
    /// A command waits for non-blocking imports in its event loop (T66).
    command_waits: bool,
    /// Number of imported functions: the IR function `i` has index `func_base + i`.
    func_base: u32,
    type_ids: HashMap<(Vec<ValType>, Vec<ValType>), u32>,
    /// Function index of the first helper (after the user functions and ABI adapters).
    helper_base: u32,
    /// Helpers in index order. Helper `k` has function index `helper_base + k`.
    helpers: Vec<Helper>,
    /// T114: `module.regexes` (`Helper::Regex`), and `module.regex_vm`.
    regexes: Vec<Vec<u16>>,
    regex_vm: bool,
    /// Interned string constants, emitted as globals after the user globals.
    /// String constants, and BigInt constants (T40): globals after the module's.
    strings: Vec<ConstGlobal>,
    string_ids: HashMap<ConstGlobal, u32>,
    /// The number → string tables (data segment 0) are needed.
    tables: bool,
    /// The code of each shared closure: its object is global `shared_base + k`.
    shared: Vec<ir::FuncId>,
    /// Functions used by `ref.func` (a declarative element segment).
    declared: Vec<u32>,
    /// The code table's functions, by slot (T89, `Cx::code_slot`).
    code_table: Vec<u32>,
    code_slots: HashMap<u32, i32>,
    /// T74: the next inline-cache site of a constant-key read (`object::Helper::GetNamed`).
    ic_sites: u32,
    /// T90: the next site of a constant-key write (`object::Helper::SetNamed`).
    set_sites: u32,
}

impl runtime::Cx for Emitter<'_> {
    fn helper(&mut self, h: runtime::Helper) -> u32 {
        self.helper_index(Helper::Js(h))
    }

    fn string(&mut self, units: &[u16]) -> u32 {
        self.const_global(ConstGlobal::Str(units.to_vec()))
    }

    fn tables_data(&mut self) -> u32 {
        self.tables = true;
        0
    }

    fn bump_global(&mut self) -> u32 {
        self.bump
    }

    fn host_globals(&mut self) -> u32 {
        self.host_base
    }

    fn lib_globals(&mut self) -> u32 {
        self.lib_base
    }

    fn command_waits(&mut self) -> bool {
        self.command_waits
    }

    fn to_int32(&mut self) -> u32 {
        self.helper_index(Helper::ToInt32)
    }

    fn imports(&mut self) -> host::Imports {
        self.imports.clone()
    }

    fn declare_func(&mut self, index: u32) {
        if !self.declared.contains(&index) {
            self.declared.push(index);
        }
    }

    fn code_slot(&mut self, index: u32) -> i32 {
        if let Some(&k) = self.code_slots.get(&index) {
            return k;
        }
        let k = self.code_table.len() as i32;
        self.code_table.push(index);
        self.code_slots.insert(index, k);
        k
    }

    fn string_to_number(&mut self) -> Option<u32> {
        self.m.string_to_number.map(|f| self.func_base + f.0)
    }

    fn bigint(&mut self) -> bool {
        self.m.bigint
    }

    fn stacks(&mut self) -> bool {
        self.m.stacks
    }

    fn rab(&mut self) -> bool {
        self.m.rab
    }

    fn subclassing(&mut self) -> bool {
        self.m.subclassing
    }

    fn ignore_unhandled(&mut self) -> bool {
        self.m.ignore_unhandled
    }

    fn cjs_main(&mut self) -> bool {
        self.m.cjs_main
    }

    fn exits(&mut self) -> bool {
        self.exits
    }

    fn counting(&mut self) -> bool {
        self.m.count
    }

    fn process_at_exit(&mut self) -> Option<(u32, bool)> {
        self.m
            .process_at_exit
            .map(|f| (self.func_base + f.0, self.m.function(f).result.is_some()))
    }

    fn promise_statics(&mut self) -> Vec<(String, u32, u32)> {
        let base = self.func_base;
        self.m
            .promise_statics
            .iter()
            .map(|m| (m.key.clone(), base + m.code.0, m.length))
            .collect()
    }

    fn symbol_statics(&mut self) -> Vec<String> {
        self.m.symbol_statics.clone()
    }

    fn big_typed(&mut self) -> bool {
        self.m.big_typed
    }

    fn to_bigint(&mut self) -> Option<u32> {
        self.m.to_bigint.map(|f| self.func_base + f.0)
    }

    fn string_to_bigint(&mut self) -> Option<u32> {
        self.m.string_to_bigint.map(|f| self.func_base + f.0)
    }

    fn js_rem(&mut self) -> u32 {
        self.helper_index(Helper::JsRem)
    }

    fn js_pow(&mut self) -> u32 {
        self.helper_index(Helper::JsPow)
    }

    fn date_call(&mut self) -> Option<u32> {
        self.m.date_call.map(|f| self.func_base + f.0)
    }

    fn date_inspect(&mut self) -> Option<u32> {
        self.m.date_inspect.map(|f| self.func_base + f.0)
    }

    fn inspect_custom(&mut self) -> Option<u32> {
        self.m.inspect_custom.map(|f| self.func_base + f.0)
    }

    fn has_maps(&mut self) -> bool {
        self.m.maps
    }

    fn has_bind(&mut self) -> bool {
        self.m.bind
    }

    fn has_to_string(&mut self) -> bool {
        self.m.to_string
    }

    fn builtin_ctor(&mut self, b: ir::Builtin) -> Option<(u32, u32)> {
        let base = self.func_base;
        self.m
            .builtin_ctors
            .iter()
            .find(|m| m.proto == b)
            .map(|m| (base + m.code.0, m.length))
    }

    fn namespace_methods(&mut self, ns: ir::Namespace) -> Vec<(String, u32, u32)> {
        let base = self.func_base;
        self.m
            .namespace_methods
            .iter()
            .filter(|(n, _)| *n == ns)
            .map(|(_, m)| (m.key.clone(), base + m.code.0, m.length))
            .collect()
    }

    fn names(&mut self, name: &str) -> bool {
        self.m.names.contains(name)
    }

    fn has_tags(&mut self) -> bool {
        let m = &self.m;
        m.maps
            || m.namespaces
            || m.promises
            || m.functions
                .iter()
                .any(|f| f.flags.is_async || f.flags.is_generator)
            || ["toStringTag", "iterator", "values", "keys", "entries"]
                .iter()
                .any(|n| m.names.contains(*n))
    }

    fn has_namespaces(&mut self) -> bool {
        self.m.namespaces
    }

    fn ctor_stubs(&mut self) -> bool {
        self.m.ctor_stubs
    }

    fn builtin_statics(&mut self, b: ir::Builtin) -> Vec<(String, u32, u32)> {
        let base = self.func_base;
        self.m
            .builtin_statics
            .iter()
            .filter(|m| m.proto == b)
            .map(|m| (m.key.clone(), base + m.code.0, m.length))
            .collect()
    }

    fn array_generic(&mut self, key: &str) -> Option<u32> {
        let base = self.func_base;
        self.m
            .array_generic
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, f)| base + f.0)
    }

    fn builtin_methods(&mut self, b: ir::Builtin) -> Vec<(String, u32, u32)> {
        let base = self.func_base;
        self.m
            .builtin_methods
            .iter()
            .filter(|m| m.proto == b)
            .map(|m| (m.key.clone(), base + m.code.0, m.length))
            .collect()
    }

    fn user_import(&mut self, i: u32) -> (u32, ir::AbiSig, Option<u32>) {
        let x = &self.m.imports[i as usize];
        let unwrap = x.unwrap.map(|f| self.func_base + f.0);
        (self.user_imports + i, x.sig.clone(), unwrap)
    }

    fn payload(&mut self, p: &ir::Payload) -> u32 {
        abi::payload_id(&self.payloads, p)
    }

    fn payload_import(&mut self, id: u32, op: abi::PayloadOp) -> u32 {
        self.payload_base + id * abi::PAYLOAD_OPS + op as u32
    }

    fn payload_elem(&mut self, id: u32) -> Option<ir::WitType> {
        self.payloads[id as usize].elem.clone()
    }

    fn payload_count(&mut self) -> u32 {
        self.payloads.len() as u32
    }

    fn payload_cancel(&mut self, id: u32) -> Option<u32> {
        self.cancel_base.map(|b| b + id)
    }

    fn payload_is_stream(&mut self, id: u32) -> bool {
        self.payloads[id as usize].stream
    }

    fn stream_funcs(&mut self) -> (u32, u32) {
        let f = |x: Option<ir::FuncId>| self.func_base + x.expect("the prelude's streams").0;
        (f(self.m.stream_reader), f(self.m.stream_pump))
    }

    fn blocking_stream_funcs(&mut self) -> (u32, (u32, bool), (u32, bool)) {
        let f = |x: Option<ir::FuncId>| {
            let x = x.expect("the prelude's streams");
            (self.func_base + x.0, self.m.function(x).result.is_some())
        };
        (
            f(self.m.stream_read_all).0,
            f(self.m.stream_defer),
            f(self.m.stream_flush),
        )
    }

    fn user_import_count(&mut self) -> u32 {
        self.m.imports.len() as u32
    }

    fn has_init(&mut self) -> bool {
        self.m.init.is_some()
    }

    fn init_func(&mut self) -> (u32, bool, bool) {
        let init = self
            .m
            .init
            .expect("only libraries with top-level code have an init");
        let jobs = self.m.exports.iter().any(|x| x.sig.jobs);
        let fails = ir::may_throw(self.m)[init.0 as usize] || jobs;
        (self.func_base + init.0, fails, jobs)
    }

    fn main_func(&mut self) -> (u32, bool) {
        let main = self.m.main.expect("host helpers are only used by commands");
        (
            self.func_base + main.0,
            self.m.function(main).result.is_some(),
        )
    }
}

impl Emitter<'_> {
    /// The global index of the first constant, after the placeholders.
    /// T109: the global holding layout `l`'s shared index.
    fn layout_global(&self, l: u32) -> u32 {
        self.global_base + (self.m.globals.len() + self.shared.len()) as u32 + l
    }

    fn const_base(&self) -> u32 {
        self.global_base
            + (self.m.globals.len() + self.shared.len() + self.m.layouts.len()) as u32
            + 1
            + self.m.bigint as u32
    }

    /// Functions that fill the constants' globals (T57), and their data segment's bytes. Each
    /// constant is `array.new_data` of its slice of the segment (UTF-16 code units, or a BigInt's
    /// words, little-endian), made by a maker per type, [`CONST_CHUNK`] constants per function.
    /// The last function is the start function: it also calls the other chunks. The functions'
    /// names are the second result.
    ///
    /// A constant expression per global (`array.new_fixed` of `i32.const`s) would need no code,
    /// but wasmtime compiles all of them as one function whose compile time grows faster than
    /// linearly with their count and length (F245).
    fn const_inits(
        &mut self,
        funcs: &mut FunctionSection,
        code: &mut CodeSection,
    ) -> (Vec<u8>, Vec<String>) {
        let mut bytes = Vec::new();
        let mut names = Vec::new();
        if self.strings.is_empty() {
            return (bytes, names);
        }
        let seg = self.tables as u32;
        let base = self.const_base();
        let ty = self.type_id(vec![], vec![]);
        // The makers, `(offset, length) -> (ref $string)` and `(ref $bigint)`: `array.new_data`
        // inline in every initializer compiles slowly (T57).
        let first = self.helper_base + self.helpers.len() as u32;
        let makers: &[(u32, &str)] = if self.m.bigint {
            &[(runtime::T_STRING, "string"), (runtime::T_BIGINT, "bigint")]
        } else {
            &[(runtime::T_STRING, "string")]
        };
        for &(t, name) in makers {
            let r = ValType::Ref(wasm_encoder::RefType {
                nullable: false,
                heap_type: wasm_encoder::HeapType::Concrete(t),
            });
            funcs.function(self.type_id(vec![ValType::I32, ValType::I32], vec![r]));
            let mut f = Function::new([]);
            f.instructions()
                .local_get(0)
                .local_get(1)
                .array_new_data(t, seg)
                .end();
            code.function(&f);
            names.push(format!("consts${name}"));
        }
        let first_chunk = first + makers.len() as u32;
        let chunks = self.strings.len().div_ceil(CONST_CHUNK);
        for (j, chunk) in self.strings.chunks(CONST_CHUNK).enumerate() {
            let mut f = Function::new([]);
            let mut s = f.instructions();
            if j + 1 == chunks && self.m.gc_reserve_mib > 0 {
                // T98: the start function first grows the GC heap once (`build --gc-reserve`): a
                // dropped byte array of that size, so later collections are rarer.
                s.i32_const((self.m.gc_reserve_mib << 20) as i32)
                    .array_new_default(runtime::T_BYTES)
                    .drop();
            }
            for (k, c) in chunk.iter().enumerate() {
                s.i32_const(bytes.len() as i32);
                let (maker, len) = match c {
                    ConstGlobal::Str(units) => {
                        bytes.extend(units.iter().flat_map(|u| u.to_le_bytes()));
                        (first, units.len())
                    }
                    ConstGlobal::BigInt(words) => {
                        bytes.extend(words.iter().flat_map(|w| w.to_le_bytes()));
                        (first + 1, words.len())
                    }
                };
                s.i32_const(len as i32)
                    .call(maker)
                    .global_set(base + (j * CONST_CHUNK + k) as u32);
            }
            if j + 1 == chunks && chunks > 1 {
                for i in 0..chunks as u32 - 1 {
                    s.call(first_chunk + i);
                }
            }
            s.end();
            funcs.function(ty);
            code.function(&f);
            names.push(format!("consts$init{j}"));
        }
        (bytes, names)
    }

    /// The global index of a constant (interned).
    fn const_global(&mut self, c: ConstGlobal) -> u32 {
        let base = self.const_base();
        if let Some(k) = self.string_ids.get(&c) {
            return base + k;
        }
        let k = self.strings.len() as u32;
        self.strings.push(c.clone());
        self.string_ids.insert(c, k);
        base + k
    }

    fn type_id(&mut self, params: Vec<ValType>, results: Vec<ValType>) -> u32 {
        if let Some(t) = self.type_ids.get(&(params.clone(), results.clone())) {
            return *t;
        }
        let t = self.next_type;
        self.next_type += 1;
        self.types.ty().function(params.clone(), results.clone());
        self.type_ids.insert((params, results), t);
        t
    }

    /// T75: `v === c` for the `jsval` `v` on the stack.
    fn strict_eq_const(&mut self, s: &mut InstructionSink, c: EqOperand) {
        let c = match c {
            EqOperand::Oddball(g) => runtime::EqConst::Oddball(g),
            EqOperand::Str(units) => runtime::EqConst::Str {
                global: runtime::Cx::string(self, units),
                len: units.len() as u32,
            },
            EqOperand::Num(v) => runtime::EqConst::Num(v),
        };
        let str_eq = self.helper_index(Helper::Js(runtime::Helper::StrEq));
        runtime::strict_eq_const(s, self.scratch_js, c, str_eq);
    }

    fn helper_index(&mut self, h: Helper) -> u32 {
        let k = match self.helpers.iter().position(|x| *x == h) {
            Some(k) => k,
            None => {
                self.helpers.push(h);
                self.helpers.len() - 1
            }
        };
        self.helper_base + k as u32
    }

    fn function(&mut self, f: &ir::Function) -> anyhow::Result<Function> {
        let mut locals: Vec<(u32, ValType)> = f.locals[f.params as usize..]
            .iter()
            .map(|l| (1, val_type(l.ty)))
            .collect();
        locals.push((1, ValType::F64));
        locals.push((1, runtime::jsval()));
        locals.push((1, runtime::rt(true, runtime::T_OBJECT)));
        locals.push((1, runtime::jsval()));
        locals.push((1, ValType::I32));
        locals.push((1, runtime::rt(true, runtime::T_VALS)));
        locals.push((1, runtime::jsval()));
        self.scratch_f64 = f.locals.len() as u32;
        self.scratch_js = self.scratch_f64 + 1;
        self.scratch_obj = self.scratch_f64 + 2;
        self.scratch_val = self.scratch_f64 + 3;
        self.scratch_i32 = self.scratch_f64 + 4;
        self.scratch_vals = self.scratch_f64 + 5;
        self.scratch_r = self.scratch_f64 + 6;
        let mut func = Function::new(locals);
        let mut frames = Vec::new();
        {
            let mut sink = func.instructions();
            for s in &f.body {
                self.stmt(&mut sink, &mut frames, s)?;
            }
            if f.result.is_some() {
                sink.unreachable();
            }
            sink.end();
        }
        Ok(func)
    }

    /// T115: `obj[key]` with a key that isn't a constant string: an element of an array (`$array`, its element
    /// store, a data value) read inline, as the `get` helper's first fast path; anything else calls `get`. An
    /// `f64` key (a typed index, `a[i]`) is tested as is and boxed only for the call.
    fn elem_get(
        &mut self,
        s: &mut wasm_encoder::InstructionSink,
        frames: &mut Vec<Frame>,
        obj: &Expr,
        key: &Expr,
    ) -> anyhow::Result<()> {
        use wasm_encoder::HeapType as Ht;
        let get = self.helper_index(Helper::Js(runtime::Helper::Obj(object::Helper::Get)));
        let (o, k, j, el, r, x) =
            (self.scratch_val, self.scratch_js, self.scratch_i32, self.scratch_vals, self.scratch_r, self.scratch_f64);
        let num = match key {
            Expr::Convert { from: Type::F64, to: Type::JsVal, arg } => Some(&**arg),
            _ => None,
        };
        self.expr(s, frames, obj)?;
        match num {
            Some(i) => {
                self.expr(s, frames, i)?;
                s.local_set(x);
            }
            None => {
                self.expr(s, frames, key)?;
                s.local_set(k);
            }
        }
        s.local_set(o);
        s.block(BlockType::Result(runtime::jsval()));
        s.block(BlockType::Empty);
        s.local_get(o).ref_test_non_null(array::array()).i32_eqz().br_if(0);
        match num {
            Some(_) => {
                // An integer-valued f64 (-0 is index 0, as ToPropertyKey says).
                s.local_get(x).i32_trunc_sat_f64_s().local_tee(j).f64_convert_i32_s();
                s.local_get(x).f64_ne().br_if(0);
            }
            None => {
                s.local_get(k).ref_test_non_null(Ht::I31).i32_eqz().br_if(0);
                s.local_get(k).ref_cast_non_null(Ht::I31).i31_get_s().local_set(j);
            }
        }
        // Inside the element store (negative: no), a data value (null: a hole or past `length`).
        s.local_get(j);
        s.local_get(o).ref_cast_non_null(array::array()).struct_get(runtime::T_ARRAY, array::F_ELEMS);
        s.local_tee(el).array_len().i32_ge_u().br_if(0);
        s.local_get(el).local_get(j).array_get(runtime::T_VALS).local_tee(r);
        s.ref_is_null().br_if(0);
        s.local_get(r).ref_test_non_null(Ht::Concrete(runtime::T_ACCESSOR)).br_if(0);
        s.local_get(r).br(1);
        s.end();
        s.local_get(o);
        match num {
            Some(_) => {
                s.local_get(x).call(self.helper_index(Helper::Js(runtime::Helper::Box)));
            }
            None => {
                s.local_get(k);
            }
        }
        s.call(get);
        s.end();
        Ok(())
    }

    fn depth(frames: &[Frame], target: Frame) -> anyhow::Result<u32> {
        match frames.iter().rev().position(|f| *f == target) {
            Some(d) => Ok(d as u32),
            None => bail!("internal error: branch target not open"),
        }
    }

    fn stmt(
        &mut self,
        s: &mut InstructionSink,
        frames: &mut Vec<Frame>,
        st: &Stmt,
    ) -> anyhow::Result<()> {
        match st {
            Stmt::Expr(e) => {
                let has_value = self.expr(s, frames, e)?;
                if has_value {
                    s.drop();
                }
            }
            Stmt::Set(l, e) => {
                self.expr(s, frames, e)?;
                s.local_set(l.0);
            }
            Stmt::SetGlobal(g, e) => {
                self.expr(s, frames, e)?;
                s.global_set(self.global_base + g.0);
            }
            Stmt::Block { label, body } => {
                s.block(BlockType::Empty);
                frames.push(Frame::Break(*label));
                for x in body {
                    self.stmt(s, frames, x)?;
                }
                frames.pop();
                s.end();
            }
            Stmt::Loop { label, body } => {
                s.block(BlockType::Empty);
                frames.push(Frame::Break(*label));
                s.loop_(BlockType::Empty);
                frames.push(Frame::Continue(*label));
                for x in body {
                    self.stmt(s, frames, x)?;
                }
                frames.pop();
                s.end();
                frames.pop();
                s.end();
            }
            Stmt::If { cond, then, els } => {
                self.expr(s, frames, cond)?;
                s.if_(BlockType::Empty);
                frames.push(Frame::Anon);
                for x in then {
                    self.stmt(s, frames, x)?;
                }
                if !els.is_empty() {
                    s.else_();
                    for x in els {
                        self.stmt(s, frames, x)?;
                    }
                }
                frames.pop();
                s.end();
            }
            Stmt::Break(l) => {
                s.br(Self::depth(frames, Frame::Break(*l))?);
            }
            Stmt::Continue(l) => {
                s.br(Self::depth(frames, Frame::Continue(*l))?);
            }
            Stmt::Return(e) => {
                if let Some(e) = e {
                    self.expr(s, frames, e)?;
                }
                s.return_();
            }
            Stmt::Throw(e) => {
                self.expr(s, frames, e)?;
                s.throw(runtime::TAG);
            }
            Stmt::Try {
                body,
                catch_local,
                catch_body,
            } => {
                // T58: a catch local is set only on the catch path, but read after the `try` (a
                // `finally`'s or `for-of`'s rethrow). Without this `null`, wasmtime's SSA construction
                // follows that read back to the function's entry value through every earlier
                // statement: translating a function with N such `try`s took O(N³) (F249).
                if let Some(l) = catch_local {
                    s.ref_null(wasm_encoder::HeapType::Abstract {
                        shared: false,
                        ty: wasm_encoder::AbstractHeapType::Eq,
                    })
                    .local_set(l.0);
                }
                s.block(BlockType::Empty);
                frames.push(Frame::Anon);
                s.block(BlockType::Result(runtime::jsval()));
                frames.push(Frame::Anon);
                // Catch labels count from outside the `try_table`: 0 is the block above.
                s.try_table(
                    BlockType::Empty,
                    [Catch::One {
                        tag: runtime::TAG,
                        label: 0,
                    }],
                );
                frames.push(Frame::Anon);
                for x in body {
                    self.stmt(s, frames, x)?;
                }
                frames.pop();
                s.end();
                s.br(1);
                frames.pop();
                s.end();
                match catch_local {
                    Some(l) => s.local_set(l.0),
                    None => s.drop(),
                };
                for x in catch_body {
                    self.stmt(s, frames, x)?;
                }
                frames.pop();
                s.end();
            }
        }
        Ok(())
    }

    /// Emit `e`. Returns whether it left a value on the stack.
    fn expr(
        &mut self,
        s: &mut InstructionSink,
        frames: &mut Vec<Frame>,
        e: &Expr,
    ) -> anyhow::Result<bool> {
        match e {
            Expr::Const(Const::F64(v)) => {
                s.f64_const((*v).into());
            }
            Expr::Const(Const::I32(v)) => {
                s.i32_const(*v);
            }
            Expr::Const(Const::Bool(v)) => {
                s.i32_const(*v as i32);
            }
            Expr::Const(Const::Undefined) => {
                s.global_get(runtime::G_UNDEFINED);
            }
            Expr::Const(Const::Null) => {
                s.global_get(runtime::G_NULL);
            }
            Expr::Local(l) => {
                s.local_get(l.0);
            }
            Expr::Global(g) => {
                s.global_get(self.global_base + g.0);
            }
            Expr::Tee(l, v) => {
                self.expr(s, frames, v)?;
                s.local_tee(l.0);
            }
            Expr::Unary { op, arg, .. } => match op {
                UnOp::BitNot => {
                    self.expr(s, frames, arg)?;
                    s.i32_const(-1).i32_xor();
                }
                UnOp::Neg => {
                    self.expr(s, frames, arg)?;
                    s.f64_neg();
                }
                UnOp::Abs => {
                    self.expr(s, frames, arg)?;
                    s.f64_abs();
                }
                UnOp::Not => {
                    self.expr(s, frames, arg)?;
                    s.i32_eqz();
                }
                UnOp::Sqrt | UnOp::Floor | UnOp::Ceil | UnOp::Trunc | UnOp::Fround | UnOp::Clz => {
                    self.expr(s, frames, arg)?;
                    match op {
                        UnOp::Sqrt => s.f64_sqrt(),
                        UnOp::Floor => s.f64_floor(),
                        UnOp::Ceil => s.f64_ceil(),
                        UnOp::Trunc => s.f64_trunc(),
                        UnOp::Fround => s.f32_demote_f64().f64_promote_f32(),
                        _ => s.i32_clz(),
                    };
                }
            },
            Expr::Binary {
                op: op @ (BinOp::Eq | BinOp::Ne),
                ty: Type::JsVal,
                lhs,
                rhs,
            } if eq_const(lhs).is_some() || eq_const(rhs).is_some() => {
                // T75: `===`/`!==` with a constant operand, inline (the constant has no effects).
                let (other, c) = match eq_const(rhs) {
                    Some(c) => (lhs, c),
                    None => (rhs, eq_const(lhs).unwrap()),
                };
                self.expr(s, frames, other)?;
                self.strict_eq_const(s, c);
                if *op == BinOp::Ne {
                    s.i32_eqz();
                }
            }
            Expr::Binary { op, ty, lhs, rhs } => {
                self.expr(s, frames, lhs)?;
                self.expr(s, frames, rhs)?;
                match ty {
                    Type::F64 => match op {
                        BinOp::Add => s.f64_add(),
                        BinOp::Sub => s.f64_sub(),
                        BinOp::Mul => s.f64_mul(),
                        BinOp::Div => s.f64_div(),
                        BinOp::Rem => s.call(self.helper_index(Helper::JsRem)),
                        BinOp::Pow => s.call(self.helper_index(Helper::JsPow)),
                        BinOp::Eq => s.f64_eq(),
                        BinOp::Ne => s.f64_ne(),
                        BinOp::Lt => s.f64_lt(),
                        BinOp::Le => s.f64_le(),
                        BinOp::Gt => s.f64_gt(),
                        BinOp::Ge => s.f64_ge(),
                        BinOp::Min => s.f64_min(),
                        BinOp::Max => s.f64_max(),
                        BinOp::CopySign => s.f64_copysign(),
                        _ => unreachable!("validator rejects {op:?} on f64"),
                    },
                    Type::I32 => match op {
                        BinOp::Add => s.i32_add(),
                        BinOp::Sub => s.i32_sub(),
                        BinOp::Mul => s.i32_mul(),
                        BinOp::Div => s.i32_div_s(),
                        BinOp::Rem => s.i32_rem_s(),
                        BinOp::Pow => unreachable!("validator rejects pow on i32"),
                        BinOp::Eq => s.i32_eq(),
                        BinOp::Ne => s.i32_ne(),
                        BinOp::Lt => s.i32_lt_s(),
                        BinOp::Le => s.i32_le_s(),
                        BinOp::Gt => s.i32_gt_s(),
                        BinOp::Ge => s.i32_ge_s(),
                        BinOp::And => s.i32_and(),
                        BinOp::Or => s.i32_or(),
                        BinOp::Xor => s.i32_xor(),
                        BinOp::Shl => s.i32_shl(),
                        BinOp::Shr => s.i32_shr_s(),
                        BinOp::ShrU => s.i32_shr_u(),
                        BinOp::LooseEq
                        | BinOp::LooseNe
                        | BinOp::Min
                        | BinOp::Max
                        | BinOp::CopySign => {
                            unreachable!("validator rejects {op:?} on i32")
                        }
                    },
                    Type::Bool => match op {
                        BinOp::Eq => s.i32_eq(),
                        BinOp::Ne => s.i32_ne(),
                        _ => unreachable!("validator rejects {op:?} on bool"),
                    },
                    Type::JsVal => {
                        let (h, negate) = match op {
                            BinOp::Eq => (runtime::Helper::StrictEq, false),
                            BinOp::Ne => (runtime::Helper::StrictEq, true),
                            BinOp::LooseEq => (runtime::Helper::LooseEq, false),
                            BinOp::LooseNe => (runtime::Helper::LooseEq, true),
                            _ => unreachable!("validator rejects {op:?} on jsval"),
                        };
                        s.call(self.helper_index(Helper::Js(h)));
                        if negate {
                            s.i32_eqz();
                        }
                        s
                    }
                    other => unreachable!("no binary ops on {other}"),
                };
            }
            Expr::Convert {
                from: Type::JsVal,
                to: Type::F64,
                arg,
            } if matches!(&**arg, Expr::Intrinsic { op: Intrinsic::GetProp, args } if !matches!(args[1], Expr::Str(_))) =>
            {
                // T107: `+o[k]` (an element read as a number): unboxed from an array in number mode.
                let Expr::Intrinsic { args, .. } = &**arg else {
                    unreachable!()
                };
                self.expr(s, frames, &args[0])?;
                self.expr(s, frames, &args[1])?;
                s.call(self.helper_index(Helper::Js(runtime::Helper::Obj(object::Helper::GetNum))));
            }
            Expr::Convert {
                from: Type::F64,
                to: Type::JsVal,
                arg,
            } if matches!(**arg, Expr::Const(Const::F64(_))) => {
                let Expr::Const(Const::F64(v)) = **arg else {
                    unreachable!()
                };
                runtime::push_number(s, v);
            }
            Expr::Convert {
                from: Type::F64,
                to: Type::JsVal,
                arg,
            } if matches!(
                **arg,
                Expr::Convert {
                    from: Type::JsVal,
                    to: Type::F64,
                    ..
                }
            ) =>
            {
                // T75: `+v` as a JS value: a number is itself, others are converted and boxed.
                let Expr::Convert { arg: v, .. } = &**arg else {
                    unreachable!()
                };
                self.expr(s, frames, v)?;
                let to_number = self.helper_index(Helper::Js(runtime::Helper::ToNumber));
                let box_ = self.helper_index(Helper::Js(runtime::Helper::Box));
                runtime::as_number_value(s, self.scratch_js, to_number, box_);
            }
            Expr::Convert { from, to, arg } => {
                self.expr(s, frames, arg)?;
                let js = |h| Helper::Js(h);
                match (from, to) {
                    (Type::F64, Type::I32) => {
                        // ToInt32, inline when |x| < 2^31 (then it's the truncation, T27).
                        let t = self.scratch_f64;
                        s.local_tee(t)
                            .f64_abs()
                            .f64_const(2147483648.0.into())
                            .f64_lt();
                        s.if_(BlockType::Result(ValType::I32));
                        s.local_get(t).i32_trunc_f64_s();
                        s.else_();
                        s.local_get(t).call(self.helper_index(Helper::ToInt32));
                        s.end();
                    }
                    (Type::I32, Type::F64) => {
                        s.f64_convert_i32_s();
                    }
                    (Type::F64, Type::JsVal) => {
                        s.call(self.helper_index(js(runtime::Helper::Box)));
                    }
                    (Type::JsVal, Type::F64) => {
                        s.call(self.helper_index(js(runtime::Helper::ToNumber)));
                    }
                    (Type::JsVal, Type::Bool) => {
                        s.call(self.helper_index(js(runtime::Helper::ToBoolean)));
                    }
                    (Type::Bool, Type::JsVal) => {
                        s.if_(BlockType::Result(runtime::jsval()));
                        s.global_get(runtime::G_TRUE);
                        s.else_();
                        s.global_get(runtime::G_FALSE);
                        s.end();
                    }
                    _ => unreachable!("validator rejects {from} -> {to}"),
                }
            }
            Expr::TypeofIs { arg, tag } => {
                self.expr(s, frames, arg)?;
                // T75: the tests that need no call, inline.
                if !runtime::typeof_is(s, self.scratch_js, *tag, self.m.bigint) {
                    s.call(self.helper_index(Helper::Js(runtime::Helper::Typeof)));
                    s.i32_const(runtime::typeof_code(*tag)).i32_eq();
                }
            }
            Expr::Seq { body, value } => {
                for x in body {
                    self.stmt(s, frames, x)?;
                }
                return self.expr(s, frames, value);
            }
            Expr::Str(units) => {
                let g = runtime::Cx::string(self, units);
                s.global_get(g);
            }
            Expr::Intrinsic { op, args } => {
                use runtime::Helper as H;
                if self.generator_op(s, frames, op, args)? {
                    return Ok(op.signature().1.is_some());
                }
                if let Intrinsic::ConsoleWrite { .. } = op
                    && self.m.main.is_none()
                {
                    bail!("internal error: console output in a module that isn't a command");
                }
                if let Intrinsic::BigIntConst = op {
                    // T40: a global made once from the literal's decimal digits.
                    let Expr::Str(units) = &args[0] else {
                        bail!("internal error: a BigInt constant needs its digits");
                    };
                    let digits = String::from_utf16_lossy(units);
                    let g = self.const_global(ConstGlobal::BigInt(bigint::from_decimal(&digits)));
                    s.global_get(g);
                    return Ok(true);
                }
                if matches!(op, Intrinsic::BigIntToNumber | Intrinsic::BigIntToString) {
                    // T40: the first argument is a BigInt (the prelude checked).
                    self.expr(s, frames, &args[0])?;
                    s.ref_cast_non_null(bigint::bigint());
                    let h = if *op == Intrinsic::BigIntToNumber {
                        bigint::Helper::ToF64
                    } else {
                        self.expr(s, frames, &args[1])?;
                        bigint::Helper::ToString
                    };
                    s.call(self.helper_index(Helper::Js(H::Big(h))));
                    return Ok(true);
                }
                if let (Intrinsic::GetProp, [obj, key]) = (op, args.as_slice())
                    && !matches!(key, Expr::Str(_))
                {
                    self.elem_get(s, frames, obj, key)?;
                    return Ok(true);
                }
                if let (Intrinsic::GetProp, [obj, Expr::Str(units)]) = (op, args.as_slice())
                    && object::named_key(units)
                {
                    // T74: a constant key: no ToPropertyKey, no element paths, the hash from the build.
                    self.expr(s, frames, obj)?;
                    let g = runtime::Cx::string(self, units);
                    // T110: a key some layout has: try its slots inline (an `$object` whose entry at a slot is
                    // `k` holds its own `k` there; keys are unique), a data value as is; else the call.
                    let slots = self.layout_slots.get(units).cloned().unwrap_or_default();
                    if !slots.is_empty() && slots.len() <= LAYOUT_GUESSES {
                        let (v, o, r) = (self.scratch_js, self.scratch_obj, self.scratch_val);
                        let on = self.m.count;
                        s.local_set(v);
                        if on {
                            s.ref_null(runtime::object()).local_set(o);
                        }
                        s.block(BlockType::Result(runtime::jsval()));
                        s.block(BlockType::Result(runtime::jsval()));
                        s.local_get(v);
                        s.br_on_cast_fail(
                            0,
                            wasm_encoder::RefType::EQREF,
                            wasm_encoder::RefType {
                                nullable: false,
                                heap_type: wasm_encoder::HeapType::Concrete(runtime::T_PLAIN),
                            },
                        );
                        s.local_set(o);
                        for &slot in &slots {
                            s.local_get(o).struct_get(runtime::T_OBJECT, object::F_COUNT);
                            s.i32_const(slot as i32).i32_gt_u();
                            s.if_(BlockType::Empty);
                            s.local_get(o).struct_get(runtime::T_OBJECT, object::F_KEYS);
                            s.i32_const(slot as i32).array_get(runtime::T_KEYS);
                            s.global_get(g).ref_eq();
                            s.if_(BlockType::Empty);
                            s.local_get(o).struct_get(runtime::T_OBJECT, object::F_VALS);
                            s.i32_const(slot as i32).array_get(runtime::T_VALS).local_tee(r);
                            s.ref_test_non_null(wasm_encoder::HeapType::Concrete(runtime::T_ACCESSOR));
                            s.local_get(r).global_get(runtime::G_BUILTIN).ref_eq();
                            s.i32_or().i32_eqz();
                            s.if_(BlockType::Empty);
                            runtime::count(s, on, "inline_read.hit");
                            s.local_get(r).br(4);
                            s.end();
                            s.end();
                            s.end();
                        }
                        s.local_get(v);
                        s.end();
                        count_miss(s, on, o, "inline_read");
                        s.global_get(g);
                        s.i32_const(object::key_hash(units));
                        s.i32_const(self.ic_sites as i32);
                        self.ic_sites += 1;
                        s.call(self.helper_index(Helper::Js(H::Obj(object::Helper::GetNamed))));
                        s.end();
                        return Ok(true);
                    }
                    s.global_get(g);
                    s.i32_const(object::key_hash(units));
                    s.i32_const(self.ic_sites as i32);
                    self.ic_sites += 1;
                    s.call(self.helper_index(Helper::Js(H::Obj(object::Helper::GetNamed))));
                    return Ok(true);
                }
                if let (Intrinsic::GetProp, [obj, Expr::Str(units)]) = (op, args.as_slice())
                    && units.as_slice() == "length".encode_utf16().collect::<Vec<_>>().as_slice()
                {
                    // T74: `v.length`.
                    self.expr(s, frames, obj)?;
                    s.call(self.helper_index(Helper::Js(H::Obj(object::Helper::GetLength))));
                    return Ok(true);
                }
                if let (Intrinsic::LayoutDefine { layout, slot }, [obj, Expr::Str(units), value]) =
                    (op, args.as_slice())
                {
                    // T109: as `DefineNamed` below, with the layout's index and the key's entry.
                    self.expr(s, frames, obj)?;
                    let g = runtime::Cx::string(self, units);
                    s.global_get(g);
                    self.expr(s, frames, value)?;
                    s.i32_const(object::key_hash(units));
                    s.global_get(self.layout_global(*layout));
                    s.i32_const(*slot as i32);
                    s.call(self.helper_index(Helper::Js(H::Obj(object::Helper::LayoutDefine))));
                    return Ok(false);
                }
                if let Intrinsic::NewObjectLayout(l) = op {
                    s.i32_const(self.m.layouts[*l as usize].len() as i32);
                    s.global_get(self.layout_global(*l));
                    s.call(self.helper_index(Helper::Js(H::Obj(object::Helper::NewObjectLayout))));
                    return Ok(true);
                }
                if let (Intrinsic::DefineProp, [obj, Expr::Str(units), value]) =
                    (op, args.as_slice())
                    && object::named_key(units)
                {
                    // T74: an object literal's constant key.
                    self.expr(s, frames, obj)?;
                    let g = runtime::Cx::string(self, units);
                    s.global_get(g);
                    self.expr(s, frames, value)?;
                    s.i32_const(object::key_hash(units));
                    s.call(self.helper_index(Helper::Js(H::Obj(object::Helper::DefineNamed))));
                    return Ok(false);
                }
                if let (Intrinsic::LayoutSet { layout, slot }, [obj, Expr::Str(units), value]) =
                    (op, args.as_slice())
                {
                    // T109: as `SetNamed` below, with the layout's index and the key's entry.
                    self.expr(s, frames, obj)?;
                    let g = runtime::Cx::string(self, units);
                    s.global_get(g);
                    self.expr(s, frames, value)?;
                    s.i32_const(object::key_hash(units));
                    s.i32_const(self.set_sites as i32);
                    self.set_sites += 1;
                    s.i32_const(self.ic_sites as i32);
                    self.ic_sites += 1;
                    s.global_get(self.layout_global(*layout));
                    s.i32_const(*slot as i32);
                    s.call(self.helper_index(Helper::Js(H::Obj(object::Helper::LayoutSet))));
                    return Ok(false);
                }
                if let Intrinsic::CreateThisLayout(l) = op {
                    self.expr(s, frames, &args[0])?;
                    s.i32_const(self.m.layouts[*l as usize].len() as i32);
                    s.global_get(self.layout_global(*l));
                    s.call(self.helper_index(Helper::Js(H::Func(func::Helper::CreateThisLayout))));
                    return Ok(true);
                }
                if let (Intrinsic::SetProp, [obj, Expr::Str(units), value]) = (op, args.as_slice())
                    && object::named_key(units)
                    && units != "__proto__".encode_utf16().collect::<Vec<_>>().as_slice()
                {
                    // T74: `o.k = v` with a constant key, as `GetNamed`.
                    self.expr(s, frames, obj)?;
                    let g = runtime::Cx::string(self, units);
                    // T110: a key some layout has: write in place at one of its slots when the entry there is
                    // `k` and a writable data property (T105's test, at a constant slot); else the call.
                    let slots = self.layout_slots.get(units).cloned().unwrap_or_default();
                    let inline = !slots.is_empty() && slots.len() <= LAYOUT_GUESSES;
                    if inline {
                        self.expr(s, frames, value)?;
                        let (v, o, x) = (self.scratch_js, self.scratch_obj, self.scratch_val);
                        let on = self.m.count;
                        s.local_set(x).local_set(v);
                        if on {
                            s.ref_null(runtime::object()).local_set(o);
                        }
                        s.block(BlockType::Empty);
                        s.block(BlockType::Result(runtime::jsval()));
                        s.local_get(v);
                        s.br_on_cast_fail(
                            0,
                            wasm_encoder::RefType::EQREF,
                            wasm_encoder::RefType {
                                nullable: false,
                                heap_type: wasm_encoder::HeapType::Concrete(runtime::T_PLAIN),
                            },
                        );
                        s.local_set(o);
                        for &slot in &slots {
                            s.local_get(o).struct_get(runtime::T_OBJECT, object::F_COUNT);
                            s.i32_const(slot as i32).i32_gt_u();
                            s.if_(BlockType::Empty);
                            s.local_get(o).struct_get(runtime::T_OBJECT, object::F_KEYS);
                            s.i32_const(slot as i32).array_get(runtime::T_KEYS);
                            s.global_get(g).ref_eq();
                            s.if_(BlockType::Empty);
                            s.local_get(o).struct_get(runtime::T_OBJECT, object::F_ATTRS);
                            s.i32_const(slot as i32).array_get_u(runtime::T_BYTES);
                            s.i32_const(object::ACCESSOR | object::READONLY | object::WATCHED);
                            s.i32_and().i32_eqz();
                            s.if_(BlockType::Empty);
                            s.local_get(o).struct_get(runtime::T_OBJECT, object::F_VALS);
                            s.i32_const(slot as i32).local_get(x).array_set(runtime::T_VALS);
                            runtime::count(s, on, "inline_write.hit");
                            s.br(4);
                            s.end();
                            s.end();
                            s.end();
                        }
                        s.local_get(v);
                        s.end();
                        count_miss(s, on, o, "inline_write");
                        s.global_get(g);
                        s.local_get(x);
                    } else {
                        s.global_get(g);
                        self.expr(s, frames, value)?;
                    }
                    s.i32_const(object::key_hash(units));
                    s.i32_const(self.set_sites as i32);
                    self.set_sites += 1;
                    // T105: and a read-style site for its entry guess.
                    s.i32_const(self.ic_sites as i32);
                    self.ic_sites += 1;
                    s.call(self.helper_index(Helper::Js(H::Obj(object::Helper::SetNamed))));
                    if inline {
                        s.end();
                    }
                    return Ok(false);
                }
                if let (
                    Intrinsic::SetProp,
                    [
                        obj,
                        key,
                        Expr::Convert {
                            from: Type::F64,
                            to: Type::JsVal,
                            arg: x,
                        },
                    ],
                ) = (op, args.as_slice())
                    && !matches!(key, Expr::Str(_))
                {
                    // T107: `o[k] = x` with a number: unboxed into an array in number mode.
                    self.expr(s, frames, obj)?;
                    self.expr(s, frames, key)?;
                    self.expr(s, frames, x)?;
                    s.call(self.helper_index(Helper::Js(H::Obj(object::Helper::SetNum))));
                    return Ok(false);
                }
                if let Intrinsic::F64FromWords = op {
                    // (hi << 32) | (lo as u32)
                    self.expr(s, frames, &args[0])?;
                    s.i64_extend_i32_u().i64_const(32).i64_shl();
                    self.expr(s, frames, &args[1])?;
                    s.i64_extend_i32_u().i64_or().f64_reinterpret_i64();
                    return Ok(true);
                }
                for (i, a) in args.iter().enumerate() {
                    self.expr(s, frames, a)?;
                    if matches!(
                        op,
                        Intrinsic::Concat
                            | Intrinsic::ConsoleWrite { .. }
                            | Intrinsic::StringLength
                    ) || (matches!(
                        op,
                        Intrinsic::StrUnit | Intrinsic::StrSub | Intrinsic::StrSet
                    ) && i == 0)
                    {
                        s.ref_cast_non_null(runtime::string());
                    }
                    if op == &Intrinsic::ReExec && i <= 1 {
                        s.ref_cast_non_null(runtime::string());
                    }
                    if op == &Intrinsic::ReExec && i == 4 {
                        s.ref_cast_non_null(wasm_encoder::HeapType::Concrete(runtime::T_I32S));
                    }
                    if matches!(
                        op,
                        Intrinsic::I32Get | Intrinsic::I32Set | Intrinsic::I32Len
                    ) && i == 0
                    {
                        s.ref_cast_non_null(wasm_encoder::HeapType::Concrete(runtime::T_I32S));
                    }
                    if matches!(op, Intrinsic::KeysLen | Intrinsic::KeysGet) && i == 0 {
                        s.ref_cast_non_null(wasm_encoder::HeapType::Concrete(runtime::T_KEYS));
                    }
                    if (matches!(
                        op,
                        Intrinsic::CallList
                            | Intrinsic::ConstructList
                            | Intrinsic::PrivateSet
                            | Intrinsic::PrivateAdd { .. }
                    ) && i == 3)
                        || (op == &Intrinsic::PrivateGet && i == 2)
                        || (op == &Intrinsic::CheckDestructure && i == 1)
                    {
                        s.ref_cast_non_null(runtime::string());
                    }
                    if i == 0 {
                        match op {
                            Intrinsic::CellGet(t) | Intrinsic::CellSet(t) => {
                                s.ref_cast_non_null(wasm_encoder::HeapType::Concrete(cell_type(
                                    *t,
                                )));
                            }
                            Intrinsic::CellIsUninit => {
                                s.ref_cast_non_null(wasm_encoder::HeapType::Concrete(
                                    runtime::T_CELL,
                                ));
                            }
                            Intrinsic::EnvGet(_) => {
                                s.ref_cast_non_null(func::function());
                            }
                            Intrinsic::ObjectDesc => {
                                s.cast_object();
                            }
                            Intrinsic::ArrayPush => {
                                s.ref_cast_non_null(array::array());
                            }
                            Intrinsic::IterValue => {
                                s.ref_cast_non_null(iter::iter());
                            }
                            _ => {}
                        }
                    }
                }
                // Inline.
                match op {
                    Intrinsic::StringLength => {
                        s.array_len().f64_convert_i32_u();
                        return Ok(true);
                    }
                    Intrinsic::KeysLen => {
                        s.array_len();
                        return Ok(true);
                    }
                    Intrinsic::KeysGet => {
                        s.array_get(runtime::T_KEYS);
                        return Ok(true);
                    }
                    Intrinsic::Prototype(b) => {
                        s.call(self.helper_index(Helper::Js(H::Obj(object::Helper::Proto(*b)))));
                        return Ok(true);
                    }
                    Intrinsic::NewCell(t) => {
                        s.struct_new(cell_type(*t));
                        return Ok(true);
                    }
                    Intrinsic::NewCellUninit => {
                        s.ref_null(wasm_encoder::HeapType::Abstract {
                            shared: false,
                            ty: wasm_encoder::AbstractHeapType::Eq,
                        })
                        .struct_new(runtime::T_CELL);
                        return Ok(true);
                    }
                    Intrinsic::CellGet(t) => {
                        s.struct_get(cell_type(*t), 0);
                        return Ok(true);
                    }
                    Intrinsic::CellSet(t) => {
                        s.struct_set(cell_type(*t), 0);
                        return Ok(false);
                    }
                    Intrinsic::CellIsUninit => {
                        s.struct_get(runtime::T_CELL, 0).ref_is_null();
                        return Ok(true);
                    }
                    Intrinsic::EnvGet(i) => {
                        s.struct_get(runtime::T_FUNCTION, func::F_ENV)
                            .i32_const(*i as i32)
                            .array_get(runtime::T_VALS);
                        return Ok(true);
                    }
                    Intrinsic::ArgGet(i) => {
                        s.i32_const(*i as i32);
                        s.call(self.helper_index(Helper::Js(H::Func(func::Helper::ArgGet))));
                        return Ok(true);
                    }
                    Intrinsic::ArgsRest(i) => {
                        s.i32_const(*i as i32);
                        s.call(self.helper_index(Helper::Js(H::Arr(array::Helper::FromArgs))));
                        return Ok(true);
                    }
                    Intrinsic::ArrayRest(i) => {
                        s.i32_const(*i as i32);
                        s.call(self.helper_index(Helper::Js(H::Arr(array::Helper::Rest))));
                        return Ok(true);
                    }
                    Intrinsic::IterValue => {
                        s.struct_get(runtime::T_ITER, 4);
                        return Ok(true);
                    }
                    Intrinsic::SymbolIterator => {
                        s.global_get(runtime::G_SYM_ITERATOR);
                        return Ok(true);
                    }
                    Intrinsic::IsArray => {
                        s.ref_test_non_null(array::array());
                        return Ok(true);
                    }
                    Intrinsic::IsPromise => {
                        s.ref_test_non_null(promise::promise());
                        return Ok(true);
                    }
                    Intrinsic::Hole => {
                        s.ref_null(wasm_encoder::HeapType::Abstract {
                            shared: false,
                            ty: wasm_encoder::AbstractHeapType::Eq,
                        });
                        return Ok(true);
                    }
                    Intrinsic::IsHole => {
                        s.ref_is_null();
                        return Ok(true);
                    }
                    Intrinsic::F64IsHole => {
                        s.i64_reinterpret_f64().i64_const(array::HOLE).i64_eq();
                        return Ok(true);
                    }
                    Intrinsic::NumOrHole => {
                        let v = self.scratch_js;
                        s.local_tee(v).ref_test_non_null(wasm_encoder::HeapType::I31);
                        s.if_(BlockType::Result(ValType::F64));
                        s.local_get(v)
                            .ref_cast_non_null(wasm_encoder::HeapType::I31)
                            .i31_get_s()
                            .f64_convert_i32_s();
                        s.else_();
                        s.local_get(v)
                            .ref_test_non_null(wasm_encoder::HeapType::Concrete(runtime::T_NUMBER));
                        s.if_(BlockType::Result(ValType::F64));
                        s.local_get(v)
                            .ref_cast_non_null(wasm_encoder::HeapType::Concrete(runtime::T_NUMBER))
                            .struct_get(runtime::T_NUMBER, 0);
                        s.else_();
                        s.i64_const(array::HOLE).f64_reinterpret_i64();
                        s.end();
                        s.end();
                        return Ok(true);
                    }
                    Intrinsic::F64Hi => {
                        s.i64_reinterpret_f64()
                            .i64_const(32)
                            .i64_shr_u()
                            .i32_wrap_i64();
                        return Ok(true);
                    }
                    Intrinsic::F64Lo => {
                        s.i64_reinterpret_f64().i32_wrap_i64();
                        return Ok(true);
                    }
                    Intrinsic::RegexId(k) => {
                        if self.regex_vm {
                            s.global_get(runtime::G_NULL);
                        } else {
                            let prog = self.regexes[*k as usize].clone();
                            let g = runtime::Cx::string(self, &prog);
                            s.global_get(g);
                        }
                        return Ok(true);
                    }
                    Intrinsic::ReExec => {
                        s.call(self.helper_index(Helper::ReExec));
                        return Ok(true);
                    }
                    Intrinsic::RandomSeed => {
                        s.call(self.imports.get(host::Import::InsecureRandom))
                            .i32_wrap_i64();
                        return Ok(true);
                    }
                    Intrinsic::MonotonicNow => {
                        s.call(self.imports.get(host::Import::ClockNow))
                            .f64_convert_i64_u();
                        return Ok(true);
                    }
                    Intrinsic::DateNow => {
                        // `now() -> instant { seconds: s64, nanoseconds: u32 }`, in memory (T26):
                        // seconds * 1000 + floor(nanoseconds / 1e6).
                        let at = host::SYSTEM_CLOCK_RESULT;
                        s.i32_const(at)
                            .call(self.imports.get(host::Import::SystemClockNow));
                        s.i32_const(at).i64_load(wasm_encoder::MemArg {
                            offset: 0,
                            align: 3,
                            memory_index: 0,
                        });
                        s.f64_convert_i64_s().f64_const(1000.0.into()).f64_mul();
                        s.i32_const(at).i32_load(wasm_encoder::MemArg {
                            offset: 8,
                            align: 2,
                            memory_index: 0,
                        });
                        s.i32_const(1_000_000)
                            .i32_div_u()
                            .f64_convert_i32_u()
                            .f64_add();
                        return Ok(true);
                    }
                    Intrinsic::CallImport(i) => {
                        let h = self.helper_index(Helper::Js(runtime::Helper::Abi(
                            abi::Helper::Import(*i),
                        )));
                        s.call(h);
                        return Ok(true);
                    }
                    Intrinsic::StrUnit => {
                        s.array_get_u(runtime::T_STRING);
                        return Ok(true);
                    }
                    Intrinsic::StrNew => {
                        s.array_new_default(runtime::T_STRING);
                        return Ok(true);
                    }
                    Intrinsic::StrSet => {
                        s.array_set(runtime::T_STRING);
                        return Ok(false);
                    }
                    Intrinsic::I32New => {
                        s.array_new_default(runtime::T_I32S);
                        return Ok(true);
                    }
                    Intrinsic::I32Get => {
                        s.array_get(runtime::T_I32S);
                        return Ok(true);
                    }
                    Intrinsic::I32Set => {
                        s.array_set(runtime::T_I32S);
                        return Ok(false);
                    }
                    Intrinsic::I32Len => {
                        s.array_len();
                        return Ok(true);
                    }
                    Intrinsic::RegExpBuiltinExec => {
                        s.global_get(runtime::G_REGEXP_EXEC).ref_eq();
                        return Ok(true);
                    }
                    Intrinsic::Trap => {
                        s.unreachable();
                        return Ok(false);
                    }
                    _ => {}
                }
                let h = match op {
                    Intrinsic::RegexId(_) | Intrinsic::ReExec => unreachable!("above"),
                    Intrinsic::JsAdd => H::JsAdd,
                    Intrinsic::ToNumeric => H::Big(bigint::Helper::ToNumeric),
                    Intrinsic::Numeric(n) => H::Big(bigint::Helper::Numeric(*n)),
                    Intrinsic::BigIntFromNumber => H::Big(bigint::Helper::FromF64),
                    Intrinsic::BigIntConst
                    | Intrinsic::BigIntToNumber
                    | Intrinsic::BigIntToString => unreachable!("above"),
                    Intrinsic::Concat => H::Concat,
                    Intrinsic::ToString => H::ToString,
                    Intrinsic::NumberToString => H::Num(numstr::Helper::NumberToString),
                    Intrinsic::JsonQuote => H::JsonQuote,
                    Intrinsic::StrCopy => H::StrCopy,
                    Intrinsic::StrBufGet => H::StrBufGet,
                    Intrinsic::StrBufAppend => H::StrBufAppend,
                    Intrinsic::StrBufLen => H::StrBufLen,
                    Intrinsic::ReAltsFind => H::ReAltsFind,
                    Intrinsic::ElemNum => H::Obj(object::Helper::ElemNum),
                    Intrinsic::JsonQuoteInto => H::JsonQuoteInto,
                    Intrinsic::JsonScan => H::JsonScan,
                    Intrinsic::StringLength
                    | Intrinsic::KeysLen
                    | Intrinsic::KeysGet
                    | Intrinsic::Prototype(_)
                    | Intrinsic::NewCell(_)
                    | Intrinsic::NewCellUninit
                    | Intrinsic::CellGet(_)
                    | Intrinsic::CellSet(_)
                    | Intrinsic::CellIsUninit
                    | Intrinsic::EnvGet(_)
                    | Intrinsic::ArgGet(_)
                    | Intrinsic::ArgsRest(_)
                    | Intrinsic::ArrayRest(_)
                    | Intrinsic::IterValue
                    | Intrinsic::SymbolIterator
                    | Intrinsic::IsArray
                    | Intrinsic::IsPromise
                    | Intrinsic::F64Hi
                    | Intrinsic::F64Lo
                    | Intrinsic::F64FromWords
                    | Intrinsic::F64IsHole
                    | Intrinsic::NumOrHole
                    | Intrinsic::RandomSeed
                    | Intrinsic::DateNow
                    | Intrinsic::MonotonicNow
                    | Intrinsic::CallImport(_)
                    | Intrinsic::StrUnit
                    | Intrinsic::StrNew
                    | Intrinsic::StrSet
                    | Intrinsic::I32New
                    | Intrinsic::I32Get
                    | Intrinsic::I32Set
                    | Intrinsic::I32Len
                    | Intrinsic::RegExpBuiltinExec
                    | Intrinsic::Trap => unreachable!("inline"),
                    Intrinsic::StrSub => H::StrSub,
                    Intrinsic::JsLess { swap, or_equal } => H::JsLess {
                        swap: *swap,
                        or_equal: *or_equal,
                    },
                    Intrinsic::ArrayPush => H::Arr(array::Helper::Push),
                    Intrinsic::ArraySpread => H::Iter(iter::Helper::Spread),
                    Intrinsic::IterOpen => H::Iter(iter::Helper::Open),
                    Intrinsic::IterStep => H::Iter(iter::Helper::Step),
                    Intrinsic::IterClose => H::Iter(iter::Helper::Close),
                    Intrinsic::IterRest => H::Iter(iter::Helper::Rest),
                    Intrinsic::AsyncIterOpen => H::Iter(iter::Helper::AsyncOpen),
                    Intrinsic::IsAsyncIter => H::Iter(iter::Helper::IsAsync),
                    Intrinsic::AsyncIterNext => H::Iter(iter::Helper::AsyncNext),
                    Intrinsic::AsyncIterResult => H::Iter(iter::Helper::AsyncResult),
                    Intrinsic::AsyncIterReturn => H::Iter(iter::Helper::AsyncReturn),
                    Intrinsic::IterHasReturn => H::Iter(iter::Helper::HasReturn),
                    Intrinsic::CallList => H::Func(func::Helper::CallList),
                    Intrinsic::ObjectRest => H::Obj(object::Helper::ObjectRest),
                    Intrinsic::CheckDestructure => H::Obj(object::Helper::CheckDestructure),
                    Intrinsic::NewSymbol => H::NewSymbol,
                    Intrinsic::ObjectKeys => H::Obj(object::Helper::Keys(0)),
                    Intrinsic::ObjectValues => H::Obj(object::Helper::Keys(1)),
                    Intrinsic::ObjectEntries => H::Obj(object::Helper::Keys(2)),
                    Intrinsic::GetOwnPropertyNames => H::Obj(object::Helper::Keys(3)),
                    Intrinsic::TypeofString => H::TypeofString,
                    Intrinsic::Inspect => H::Inspect,
                    Intrinsic::NoFormat => H::NoFormat,
                    Intrinsic::ConsoleWrite { stderr } => {
                        H::Host(host::Helper::Append(*stderr as u32))
                    }
                    Intrinsic::Environment(k) => H::Host(host::Helper::Environment(*k)),
                    Intrinsic::ProcessExit => H::Host(host::Helper::Exit),
                    Intrinsic::NewError(k) => H::Obj(object::Helper::NewError(*k)),
                    Intrinsic::NewObject => H::Obj(object::Helper::NewObject),
                    Intrinsic::NewObjectSized => H::Obj(object::Helper::NewObjectSized),
                    Intrinsic::GetProp => H::Obj(object::Helper::Get),
                    Intrinsic::SetProp => H::Obj(object::Helper::Set),
                    Intrinsic::DefineProp => H::Obj(object::Helper::DefineProp),
                    Intrinsic::DeleteProp => H::Obj(object::Helper::Delete),
                    Intrinsic::HasProp => H::Obj(object::Helper::Has),
                    Intrinsic::SetProto => H::Obj(object::Helper::SetProto),
                    Intrinsic::CopyProps => H::Obj(object::Helper::CopyProps),
                    Intrinsic::ForInKeys => H::Obj(object::Helper::ForInKeys),
                    Intrinsic::ObjectCreate => H::Obj(object::Helper::Create),
                    Intrinsic::GetPrototypeOf => H::Obj(object::Helper::GetPrototypeOf),
                    Intrinsic::GetOwnPropertySymbols => H::Obj(object::Helper::GetOwnSymbols),
                    Intrinsic::BuiltinTag => H::Obj(object::Helper::TagOf),
                    Intrinsic::ToObject => H::Wrap(wrapper::Helper::ToObject),
                    Intrinsic::Unwrap => H::Wrap(wrapper::Helper::Unwrap),
                    Intrinsic::RegExpNew => H::Obj(object::Helper::RegExpNew),
                    Intrinsic::RegExpPlain => H::Obj(object::Helper::RegExpPlain),
                    Intrinsic::RegExpData => H::Obj(object::Helper::RegExpData),
                    Intrinsic::StackNew(a) => H::Obj(object::Helper::StackNew(*a)),
                    Intrinsic::StackData => H::Obj(object::Helper::StackData),
                    Intrinsic::DateNew => H::Obj(object::Helper::DateNew),
                    Intrinsic::DateData => H::Obj(object::Helper::DateData),
                    Intrinsic::ModuleNew => H::Obj(object::Helper::ModuleNew),
                    Intrinsic::ToPrimitive => H::Obj(object::Helper::ToPrimitiveValue),
                    Intrinsic::InspectAt(d) => H::Fmt(inspect::Helper::At(*d)),
                    Intrinsic::InspectDepth => H::Fmt(inspect::Helper::AtDepth),
                    Intrinsic::BuiltinToString => H::Obj(object::Helper::BuiltinToString),
                    Intrinsic::IsConstructor => H::Func(func::Helper::IsConstructor),
                    Intrinsic::WellKnownSymbol(k) => H::WellKnownSymbol(*k),
                    Intrinsic::HasOwn => H::Obj(object::Helper::HasOwn),
                    Intrinsic::InstanceOf(b) => H::Obj(object::Helper::InstanceOf(*b)),
                    Intrinsic::ConstructList => H::Func(func::Helper::ConstructList),
                    Intrinsic::CreateThis => H::Func(func::Helper::CreateThis),
                    Intrinsic::CreateThisSized => H::Func(func::Helper::CreateThisSized),
                    Intrinsic::CreateThisLayout(_)
                    | Intrinsic::LayoutSet { .. }
                    | Intrinsic::NewObjectLayout(_)
                    | Intrinsic::LayoutDefine { .. } => {
                        unreachable!("emitted above")
                    }
                    Intrinsic::ClassHeritage => H::Func(func::Helper::ClassHeritage),
                    Intrinsic::ClassSetup => H::Func(func::Helper::ClassSetup),
                    Intrinsic::InstanceOfValue => H::Func(func::Helper::InstanceOfValue),
                    Intrinsic::CtorReturn { derived } => {
                        H::Func(func::Helper::CtorReturn { derived: *derived })
                    }
                    Intrinsic::DefineMethod => H::Prop(props::Helper::DefineData(object::HIDDEN)),
                    Intrinsic::DefineField => H::Prop(props::Helper::DefineData(0)),
                    Intrinsic::DefineAccessor { setter, enumerable } => {
                        s.i32_const(*setter as i32);
                        s.i32_const(if *enumerable { 0 } else { object::HIDDEN });
                        H::Prop(props::Helper::DefineAccessor)
                    }
                    Intrinsic::SuperGet => H::Prop(props::Helper::SuperGet),
                    Intrinsic::SuperSet => H::Prop(props::Helper::SuperSet),
                    Intrinsic::DefineProperty => H::Prop(props::Helper::DefineProperty),
                    Intrinsic::GetOwnPropertyDescriptor => {
                        H::Prop(props::Helper::GetOwnPropertyDescriptor)
                    }
                    Intrinsic::Integrity(op) => H::Prop(props::Helper::Integrity(*op)),
                    Intrinsic::TestIntegrity(op) => H::Prop(props::Helper::TestIntegrity(*op)),
                    Intrinsic::ErrorCtor(k) => H::Func(func::Helper::ErrorCtor(*k)),
                    Intrinsic::PromiseCtor => H::Prom(promise::Helper::Ctor),
                    Intrinsic::QueueMicrotask => H::Prom(promise::Helper::QueueMicrotask),
                    Intrinsic::Timer(k) => H::Timer(timers::Helper::Fn(*k)),
                    Intrinsic::Map(op) => H::Map(mapset::Helper::Op(*op)),
                    Intrinsic::Typed(op) => H::Typed(typed::Helper::Op(*op)),
                    Intrinsic::Stream(op) => H::Abi(abi::Helper::Stream(*op)),
                    Intrinsic::ImportCancel => H::Abi(abi::Helper::ImportCancel),
                    Intrinsic::MakeArguments => H::Obj(object::Helper::MakeArguments),
                    Intrinsic::SetFunctionName => H::Func(func::Helper::SetName),
                    Intrinsic::BuiltinCtor(b) => H::Func(func::Helper::BuiltinCtor(*b)),
                    Intrinsic::Namespace(ns) => H::Func(func::Helper::Namespace(*ns)),
                    Intrinsic::PrivateGet => H::Prop(props::Helper::PrivateGet),
                    Intrinsic::PrivateSet => H::Prop(props::Helper::PrivateSet),
                    Intrinsic::PrivateHas => H::Prop(props::Helper::PrivateHas),
                    Intrinsic::PrivateAdd { kind } => H::Prop(props::Helper::PrivateAdd(*kind)),
                    Intrinsic::NewAccessor => H::Prop(props::Helper::NewAccessor),
                    Intrinsic::ToPropertyKey => H::Obj(object::Helper::ToKey),
                    Intrinsic::ObjectDesc => H::Prop(props::Helper::Desc),
                    Intrinsic::Hole | Intrinsic::IsHole => unreachable!("inline"),
                    Intrinsic::ResumeMode
                    | Intrinsic::NewGenerator { .. }
                    | Intrinsic::NewAsync { .. }
                    | Intrinsic::NewAsyncGenerator { .. }
                    | Intrinsic::AsyncStart
                    | Intrinsic::GenLoad(_)
                    | Intrinsic::GenStore(_)
                    | Intrinsic::GenPoint
                    | Intrinsic::GenSuspend { .. }
                    | Intrinsic::DelegateOpen
                    | Intrinsic::DelegateStep => unreachable!("generator_op"),
                };
                s.call(self.helper_index(Helper::Js(h)));
                return Ok(op.signature().1.is_some());
            }
            Expr::Cond {
                cond,
                then,
                els,
                ty,
            } => {
                self.expr(s, frames, cond)?;
                s.if_(BlockType::Result(val_type(*ty)));
                frames.push(Frame::Anon);
                self.expr(s, frames, then)?;
                s.else_();
                self.expr(s, frames, els)?;
                frames.pop();
                s.end();
            }
            Expr::Call { func, args } => {
                for a in args {
                    self.expr(s, frames, a)?;
                }
                s.call(self.func_base + func.0);
                return Ok(self.m.function(*func).result.is_some());
            }
            Expr::Suspend { .. } => unreachable!("rejected by check_supported"),
            Expr::Closure(c) => {
                let shared = c
                    .shared
                    .then(|| self.shared.iter().position(|x| *x == c.code).unwrap() as u32)
                    .map(|k| self.global_base + self.m.globals.len() as u32 + k);
                if let Some(g) = shared {
                    s.global_get(g).ref_is_null();
                    s.if_(BlockType::Empty);
                    frames.push(Frame::Anon);
                }
                let code = self.func_base + c.code.0;
                let slot = runtime::Cx::code_slot(self, code);
                s.i32_const(slot);
                let flags = match c.kind {
                    ir::FnKind::Plain => {
                        s.i32_const(-1);
                        0
                    }
                    ir::FnKind::Ordinary => {
                        s.i32_const(-1);
                        func::NEW_ORDINARY
                    }
                    ir::FnKind::OrdinaryWith { construct } => {
                        let construct = self.func_base + construct.0;
                        let slot = runtime::Cx::code_slot(self, construct);
                        s.i32_const(slot);
                        func::NEW_ORDINARY
                    }
                    ir::FnKind::Generator => {
                        s.i32_const(-1);
                        func::NEW_GENERATOR
                    }
                    ir::FnKind::Async => {
                        s.i32_const(-1);
                        func::NEW_ASYNC
                    }
                    ir::FnKind::AsyncGenerator => {
                        s.i32_const(-1);
                        func::NEW_ASYNC_GENERATOR
                    }
                    ir::FnKind::Class { construct } => {
                        let construct = self.func_base + construct.0;
                        let slot = runtime::Cx::code_slot(self, construct);
                        s.i32_const(slot);
                        func::NEW_CLASS
                    }
                };
                if c.env.is_empty() {
                    s.ref_null(func::vals());
                } else {
                    for x in &c.env {
                        self.expr(s, frames, x)?;
                    }
                    s.array_new_fixed(runtime::T_VALS, c.env.len() as u32);
                }
                let name = runtime::Cx::str(self, &c.name);
                s.global_get(name);
                s.i32_const(c.length as i32).i32_const(flags);
                s.call(self.helper_index(Helper::Js(runtime::Helper::Func(func::Helper::New))));
                if let Some(src) = &c.source {
                    let src = runtime::Cx::str(self, src);
                    s.global_get(src);
                    s.call(
                        self.helper_index(Helper::Js(runtime::Helper::Func(
                            func::Helper::SetSource,
                        ))),
                    );
                }
                if let Some(g) = shared {
                    s.global_set(g);
                    frames.pop();
                    s.end();
                    s.global_get(g);
                }
            }
            Expr::Array(elems) => {
                for x in elems {
                    match x {
                        Some(x) => {
                            self.expr(s, frames, x)?;
                        }
                        None => {
                            s.ref_null(wasm_encoder::RefType::EQREF.heap_type);
                        }
                    }
                }
                s.array_new_fixed(runtime::T_VALS, elems.len() as u32);
                s.i32_const(elems.len() as i32);
                // T107: the number-mode state from the elements' IR where it's known.
                let i31_const = |x: &Expr| match x {
                    Expr::Convert { from: Type::F64, to: Type::JsVal, arg } => {
                        matches!(**arg, Expr::Const(Const::F64(v)) if runtime::small_int(v).is_some())
                    }
                    _ => false,
                };
                let number = |x: &Expr| {
                    matches!(x, Expr::Convert { from: Type::F64, to: Type::JsVal, .. })
                };
                let h = if elems.iter().all(|x| x.as_ref().is_some_and(i31_const)) {
                    array::Helper::NewHinted
                } else if elems.iter().all(|x| x.as_ref().is_some_and(number)) {
                    array::Helper::New
                } else {
                    array::Helper::NewGeneric
                };
                s.call(self.helper_index(Helper::Js(runtime::Helper::Arr(h))));
            }
            Expr::CallRef {
                callee,
                this,
                args,
                what,
            } => {
                self.expr(s, frames, callee)?;
                self.expr(s, frames, this)?;
                for a in args {
                    self.expr(s, frames, a)?;
                }
                s.array_new_fixed(runtime::T_VALS, args.len() as u32);
                let what = runtime::Cx::str(self, what);
                s.global_get(what);
                s.call(
                    self.helper_index(Helper::Js(runtime::Helper::Func(func::Helper::CallValue))),
                );
            }
            Expr::New {
                callee,
                new_target,
                args,
                what,
            } => {
                self.expr(s, frames, callee)?;
                for a in args {
                    self.expr(s, frames, a)?;
                }
                s.array_new_fixed(runtime::T_VALS, args.len() as u32);
                let h = match new_target {
                    Some(nt) => {
                        self.expr(s, frames, nt)?;
                        func::Helper::Construct
                    }
                    None => func::Helper::ConstructSame,
                };
                let what = runtime::Cx::str(self, what);
                s.global_get(what);
                s.call(self.helper_index(Helper::Js(runtime::Helper::Func(h))));
            }
        }
        Ok(true)
    }

    /// The generator intrinsics (T18); `false` for the others.
    fn generator_op(
        &mut self,
        s: &mut InstructionSink,
        frames: &mut Vec<Frame>,
        op: &Intrinsic,
        args: &[Expr],
    ) -> anyhow::Result<bool> {
        use runtime::Helper as H;
        let gen_arg = |this: &mut Self, s: &mut InstructionSink, frames: &mut Vec<Frame>| {
            this.expr(s, frames, &args[0])?;
            s.ref_cast_non_null(generator::generator());
            anyhow::Ok(())
        };
        match op {
            Intrinsic::ResumeMode => {
                bail!("internal error: ResumeMode after the suspension transform")
            }
            Intrinsic::NewGenerator { resume, slots }
            | Intrinsic::NewAsync { resume, slots }
            | Intrinsic::NewAsyncGenerator { resume, slots } => {
                let code = self.func_base + resume.0;
                runtime::Cx::declare_func(self, code);
                s.ref_func(code).i32_const(*slots as i32);
                let h = match op {
                    Intrinsic::NewAsync { .. } => H::Async(asyncfn::Helper::New),
                    Intrinsic::NewAsyncGenerator { .. } => H::AsyncGen(asyncgen::Helper::New),
                    _ => H::Gen(generator::Helper::New),
                };
                s.call(self.helper_index(Helper::Js(h)));
            }
            Intrinsic::AsyncStart => {
                self.expr(s, frames, &args[0])?;
                s.call(self.helper_index(Helper::Js(H::Async(asyncfn::Helper::Start))));
            }
            Intrinsic::GenLoad(i) => {
                gen_arg(self, s, frames)?;
                s.struct_get(runtime::T_GENERATOR, generator::F_FRAME)
                    .i32_const(*i as i32)
                    .array_get(runtime::T_VALS);
            }
            Intrinsic::GenStore(i) => {
                gen_arg(self, s, frames)?;
                s.struct_get(runtime::T_GENERATOR, generator::F_FRAME)
                    .i32_const(*i as i32);
                self.expr(s, frames, &args[1])?;
                s.array_set(runtime::T_VALS);
            }
            Intrinsic::GenPoint => {
                gen_arg(self, s, frames)?;
                s.struct_get(runtime::T_GENERATOR, generator::F_POINT);
            }
            Intrinsic::GenSuspend { raw, awaiting } => {
                self.expr(s, frames, &args[0])?;
                self.expr(s, frames, &args[1])?;
                s.i32_const(if *awaiting {
                    asyncgen::YIELDED_AWAIT
                } else if *raw {
                    2
                } else {
                    1
                });
                s.call(self.helper_index(Helper::Js(H::Gen(generator::Helper::Suspend))));
            }
            Intrinsic::DelegateOpen | Intrinsic::DelegateStep => {
                for a in args {
                    self.expr(s, frames, a)?;
                }
                let h = if *op == Intrinsic::DelegateOpen {
                    generator::Helper::DelegateOpen
                } else {
                    generator::Helper::DelegateStep
                };
                s.call(self.helper_index(Helper::Js(H::Gen(h))));
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn helper(&mut self, h: Helper) -> Function {
        match h {
            Helper::JsRem => js_rem(),
            Helper::JsPow => pow::js_pow(),
            Helper::ToInt32 => {
                let rem = self.helper_index(Helper::JsRem);
                to_int32(rem)
            }
            Helper::Js(h) => h.body(self),
            Helper::ReExec => {
                let wide = self.helper_index(Helper::ReClassWide);
                let grow = self.helper_index(Helper::ReGrow);
                regex::exec(wide, grow)
            }
            Helper::ReClassWide => regex::class_wide(),
            Helper::ReGrow => regex::grow(),
        }
    }
}

/// `$to_int32(x: f64) -> i32`: JS ToInt32.
fn to_int32(js_rem: u32) -> Function {
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(0)
        .f64_abs()
        .f64_const(9223372036854775808.0.into())
        .f64_lt();
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(0).i64_trunc_sat_f64_s().i32_wrap_i64();
    s.else_();
    // |x| ≥ 2^63, NaN or ±∞: reduce modulo 2^32 exactly first (NaN/∞ → NaN → 0).
    s.local_get(0)
        .f64_const(4294967296.0.into())
        .call(js_rem)
        .i64_trunc_sat_f64_s()
        .i32_wrap_i64();
    s.end();
    s.end();
    f
}

/// `$js_rem(x: f64, y: f64) -> f64`: exact `fmod`, the semantics of JS `%` on numbers.
/// A port of musl's `fmod` (bit manipulation, no rounding error).
fn js_rem() -> Function {
    const X: u32 = 0;
    const Y: u32 = 1;
    const UX: u32 = 2;
    const UY: u32 = 3;
    const EX: u32 = 4;
    const EY: u32 = 5;
    const SX: u32 = 6;
    const I: u32 = 7;
    let mut f = Function::new([(6, ValType::I64)]);
    let mut s = f.instructions();

    // ux = bits(x); uy = bits(y); ex = ux>>52 & 0x7ff; ey = uy>>52 & 0x7ff; sx = ux>>63
    s.local_get(X).i64_reinterpret_f64().local_set(UX);
    s.local_get(Y).i64_reinterpret_f64().local_set(UY);
    s.local_get(UX)
        .i64_const(52)
        .i64_shr_u()
        .i64_const(0x7ff)
        .i64_and()
        .local_set(EX);
    s.local_get(UY)
        .i64_const(52)
        .i64_shr_u()
        .i64_const(0x7ff)
        .i64_and()
        .local_set(EY);
    s.local_get(UX).i64_const(63).i64_shr_u().local_set(SX);

    // if (uy<<1 == 0 || isnan(y) || ex == 0x7ff) return (x*y)/(x*y)
    s.local_get(UY).i64_const(1).i64_shl().i64_eqz();
    s.local_get(Y).local_get(Y).f64_ne().i32_or();
    s.local_get(EX).i64_const(0x7ff).i64_eq().i32_or();
    s.if_(BlockType::Empty);
    s.local_get(X)
        .local_get(Y)
        .f64_mul()
        .local_get(X)
        .local_get(Y)
        .f64_mul()
        .f64_div()
        .return_();
    s.end();

    // if (ux<<1 <= uy<<1) { if (ux<<1 == uy<<1) return 0*x; return x; }
    s.local_get(UX)
        .i64_const(1)
        .i64_shl()
        .local_get(UY)
        .i64_const(1)
        .i64_shl()
        .i64_le_u();
    s.if_(BlockType::Empty);
    s.local_get(UX)
        .i64_const(1)
        .i64_shl()
        .local_get(UY)
        .i64_const(1)
        .i64_shl()
        .i64_eq();
    s.if_(BlockType::Empty);
    s.f64_const(0.0.into()).local_get(X).f64_mul().return_();
    s.end();
    s.local_get(X).return_();
    s.end();

    // Normalize x and y (subnormals: shift the leading 1 into bit 52).
    for (u, e) in [(UX, EX), (UY, EY)] {
        s.local_get(e).i64_eqz();
        s.if_(BlockType::Empty);
        {
            // for (i = u<<12; i>>63 == 0; e--, i <<= 1);
            s.local_get(u).i64_const(12).i64_shl().local_set(I);
            s.block(BlockType::Empty).loop_(BlockType::Empty);
            s.local_get(I)
                .i64_const(63)
                .i64_shr_u()
                .i32_wrap_i64()
                .br_if(1);
            s.local_get(e).i64_const(1).i64_sub().local_set(e);
            s.local_get(I).i64_const(1).i64_shl().local_set(I);
            s.br(0).end().end();
            // u <<= 1 - e
            s.local_get(u)
                .i64_const(1)
                .local_get(e)
                .i64_sub()
                .i64_shl()
                .local_set(u);
        }
        s.else_();
        {
            // u = (u & (2^52 - 1)) | 2^52
            s.local_get(u)
                .i64_const(0x000f_ffff_ffff_ffff)
                .i64_and()
                .i64_const(1 << 52)
                .i64_or()
                .local_set(u);
        }
        s.end();
    }

    // Shared step: i = ux - uy; if (i >= 0) { if (i == 0) return 0*x; ux = i; }
    let step = |s: &mut InstructionSink| {
        s.local_get(UX)
            .local_get(UY)
            .i64_sub()
            .local_tee(I)
            .i64_const(0)
            .i64_ge_s();
        s.if_(BlockType::Empty);
        s.local_get(I).i64_eqz();
        s.if_(BlockType::Empty);
        s.f64_const(0.0.into()).local_get(X).f64_mul().return_();
        s.end();
        s.local_get(I).local_set(UX);
        s.end();
    };

    // for (; ex > ey; ex--) { step; ux <<= 1; }
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(EX).local_get(EY).i64_le_s().br_if(1);
    step(&mut s);
    s.local_get(UX).i64_const(1).i64_shl().local_set(UX);
    s.local_get(EX).i64_const(1).i64_sub().local_set(EX);
    s.br(0).end().end();
    step(&mut s);

    // for (; ux>>52 == 0; ux <<= 1, ex--);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(UX)
        .i64_const(52)
        .i64_shr_u()
        .i64_eqz()
        .i32_eqz()
        .br_if(1);
    s.local_get(UX).i64_const(1).i64_shl().local_set(UX);
    s.local_get(EX).i64_const(1).i64_sub().local_set(EX);
    s.br(0).end().end();

    // if (ex > 0) { ux -= 2^52; ux |= ex << 52; } else { ux >>= 1 - ex; }
    s.local_get(EX).i64_const(0).i64_gt_s();
    s.if_(BlockType::Empty);
    s.local_get(UX)
        .i64_const(1 << 52)
        .i64_sub()
        .local_get(EX)
        .i64_const(52)
        .i64_shl()
        .i64_or()
        .local_set(UX);
    s.else_();
    s.local_get(UX)
        .i64_const(1)
        .local_get(EX)
        .i64_sub()
        .i64_shr_u()
        .local_set(UX);
    s.end();

    // return bits(ux | sx << 63)
    s.local_get(UX)
        .local_get(SX)
        .i64_const(63)
        .i64_shl()
        .i64_or()
        .f64_reinterpret_i64();
    s.end();
    f
}
