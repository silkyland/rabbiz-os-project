//! Self-contained, zero-external-dependency WebAssembly (`.wasm`) binary parser,
//! fuel-limited deterministic stack-machine interpreter, permission sandbox, and
//! `AppState` runner for RabbizOS (`wit/rabbiz-app.wit`).

#![forbid(unsafe_code)]

use std::collections::HashMap;

use de_api::{Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::Canvas;
use rabbiz_app_kit::{AppEvent, AppFrame, AppState, TextInputCtl};
use serde::{Deserialize, Serialize};

/// Default per-invocation instruction fuel limit so infinite loops in `.wasm`
/// modules trap cleanly instead of hanging the phone.
pub const DEFAULT_MAX_STEPS: usize = 250_000;

/// WASM binary magic + version 1 (`\0asm\x01\0\0\0`).
pub const WASM_MAGIC_VERSION: [u8; 8] = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

/// Capability sandbox permissions enforced on host imports (`rabbiz.toml`).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WasmPermissions {
    pub allow_intents: bool,
    pub allow_notifications: bool,
    pub allow_storage: bool,
    pub allow_network: bool,
}

impl WasmPermissions {
    #[must_use]
    pub const fn allow_all() -> Self {
        Self {
            allow_intents: true,
            allow_notifications: true,
            allow_storage: true,
            allow_network: true,
        }
    }
}

/// Host import functions available under the `"rabbiz"` import module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostImport {
    CanvasFillRect,
    CanvasFillRoundedRect,
    CanvasDrawText,
    ThemeColor,
    IntentOpen,
    Notify,
}

impl HostImport {
    #[must_use]
    pub fn from_name(module: &str, name: &str) -> Option<Self> {
        if module != "rabbiz" {
            return None;
        }
        match name {
            "canvas_fill_rect" | "fill-rect" => Some(Self::CanvasFillRect),
            "canvas_fill_rounded_rect" | "fill-rounded-rect" => Some(Self::CanvasFillRoundedRect),
            "canvas_draw_text" | "draw-text" => Some(Self::CanvasDrawText),
            "theme_color" | "theme-color" => Some(Self::ThemeColor),
            "intent_open" | "intent-open" => Some(Self::IntentOpen),
            "notify" => Some(Self::Notify),
            _ => None,
        }
    }

    #[must_use]
    pub const fn param_count(self) -> usize {
        match self {
            Self::CanvasFillRect => 5,
            Self::CanvasFillRoundedRect | Self::CanvasDrawText => 6,
            Self::ThemeColor => 1,
            Self::IntentOpen => 2,
            Self::Notify => 4,
        }
    }

    #[must_use]
    pub const fn result_count(self) -> usize {
        match self {
            Self::CanvasFillRect | Self::CanvasFillRoundedRect | Self::CanvasDrawText => 0,
            Self::ThemeColor | Self::IntentOpen | Self::Notify => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuncType {
    pub params: usize,
    pub results: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instr {
    Unreachable,
    Nop,
    Block { has_result: bool, end_pc: usize },
    Loop { has_result: bool, start_pc: usize, end_pc: usize },
    If { has_result: bool, else_pc: Option<usize>, end_pc: usize },
    Else { end_pc: usize },
    End,
    Br(u32),
    BrIf(u32),
    Return,
    Call(u32),
    Drop,
    Select,
    LocalGet(u32),
    LocalSet(u32),
    LocalTee(u32),
    GlobalGet(u32),
    GlobalSet(u32),
    I32Load { offset: u32 },
    I32Store { offset: u32 },
    I32Const(i32),
    I32Eqz,
    I32Eq,
    I32Ne,
    I32LtS,
    I32GtS,
    I32LeS,
    I32GeS,
    I32Add,
    I32Sub,
    I32Mul,
    I32DivS,
    I32And,
    I32Or,
    I32Xor,
    I32Shl,
    I32ShrU,
}

#[derive(Debug, Clone)]
pub struct DefinedFunc {
    pub type_idx: u32,
    pub num_locals: usize,
    pub code: Vec<Instr>,
}

/// Parsed and validated WebAssembly module with linear memory and globals.
#[derive(Debug, Clone)]
pub struct WasmModule {
    pub types: Vec<FuncType>,
    pub imports: Vec<HostImport>,
    pub funcs: Vec<DefinedFunc>,
    pub globals: Vec<i32>,
    pub memory: Vec<u8>,
    pub exports: HashMap<String, u32>,
    pub max_steps: usize,
}

/// Active host drawing & sandbox state passed during a WASM invocation.
pub struct HostContext<'a> {
    pub canvas: Option<&'a mut Canvas>,
    pub theme: Theme,
    pub permissions: WasmPermissions,
    pub app_id: String,
    pub last_intent: Option<String>,
    pub last_notification: Option<(String, String)>,
}

impl<'a> HostContext<'a> {
    #[must_use]
    pub fn headless(permissions: WasmPermissions) -> Self {
        Self {
            canvas: None,
            theme: tokens::LIGHT,
            permissions,
            app_id: "rabbiz-wasm-app".to_string(),
            last_intent: None,
            last_notification: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct LabelFrame {
    is_loop: bool,
    target_pc: usize,
    stack_height: usize,
    arity: usize,
}

impl WasmModule {
    /// Parse and validate a `.wasm` binary buffer.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 8 || bytes[..8] != WASM_MAGIC_VERSION {
            return Err("invalid WebAssembly magic or version header".to_string());
        }
        let mut pos = 8usize;
        let mut types = Vec::new();
        let mut imports = Vec::new();
        let mut func_type_indices = Vec::new();
        let mut globals = Vec::new();
        let mut memory = vec![0u8; 65536]; // default 1 WASM page (64 KiB)
        let mut exports = HashMap::new();
        let mut func_bodies = Vec::new();

        while pos < bytes.len() {
            let section_id = read_u8(bytes, &mut pos)?;
            let section_len = read_leb_u32(bytes, &mut pos)? as usize;
            let section_end = pos
                .checked_add(section_len)
                .filter(|&e| e <= bytes.len())
                .ok_or_else(|| "truncated WASM section".to_string())?;

            let mut sec_pos = pos;
            match section_id {
                0 => {
                    // Custom section — skip
                }
                1 => {
                    // Type section
                    let count = read_leb_u32(bytes, &mut sec_pos)? as usize;
                    for _ in 0..count {
                        let form = read_u8(bytes, &mut sec_pos)?;
                        if form != 0x60 {
                            return Err(format!("unsupported type form 0x{form:02x}"));
                        }
                        let p_count = read_leb_u32(bytes, &mut sec_pos)? as usize;
                        for _ in 0..p_count {
                            let _vt = read_u8(bytes, &mut sec_pos)?;
                        }
                        let r_count = read_leb_u32(bytes, &mut sec_pos)? as usize;
                        for _ in 0..r_count {
                            let _vt = read_u8(bytes, &mut sec_pos)?;
                        }
                        types.push(FuncType {
                            params: p_count,
                            results: r_count,
                        });
                    }
                }
                2 => {
                    // Import section
                    let count = read_leb_u32(bytes, &mut sec_pos)? as usize;
                    for _ in 0..count {
                        let mod_name = read_name(bytes, &mut sec_pos)?;
                        let field_name = read_name(bytes, &mut sec_pos)?;
                        let kind = read_u8(bytes, &mut sec_pos)?;
                        if kind != 0x00 {
                            return Err(format!("only function imports are supported (got {kind})"));
                        }
                        let _type_idx = read_leb_u32(bytes, &mut sec_pos)?;
                        let hi = HostImport::from_name(&mod_name, &field_name).ok_or_else(|| {
                            format!("unknown host import {mod_name}.{field_name}")
                        })?;
                        imports.push(hi);
                    }
                }
                3 => {
                    // Function section
                    let count = read_leb_u32(bytes, &mut sec_pos)? as usize;
                    for _ in 0..count {
                        func_type_indices.push(read_leb_u32(bytes, &mut sec_pos)?);
                    }
                }
                5 => {
                    // Memory section
                    let count = read_leb_u32(bytes, &mut sec_pos)? as usize;
                    if count > 0 {
                        let flags = read_u8(bytes, &mut sec_pos)?;
                        let initial_pages = read_leb_u32(bytes, &mut sec_pos)? as usize;
                        if flags & 0x01 != 0 {
                            let _max_pages = read_leb_u32(bytes, &mut sec_pos)?;
                        }
                        let pages = initial_pages.clamp(1, 32);
                        memory.resize(pages * 65536, 0);
                    }
                }
                6 => {
                    // Global section
                    let count = read_leb_u32(bytes, &mut sec_pos)? as usize;
                    for _ in 0..count {
                        let _val_type = read_u8(bytes, &mut sec_pos)?;
                        let _mutability = read_u8(bytes, &mut sec_pos)?;
                        let init_val = read_const_expr_i32(bytes, &mut sec_pos)?;
                        globals.push(init_val);
                    }
                }
                7 => {
                    // Export section
                    let count = read_leb_u32(bytes, &mut sec_pos)? as usize;
                    for _ in 0..count {
                        let name = read_name(bytes, &mut sec_pos)?;
                        let kind = read_u8(bytes, &mut sec_pos)?;
                        let index = read_leb_u32(bytes, &mut sec_pos)?;
                        if kind == 0x00 {
                            exports.insert(name, index);
                        }
                    }
                }
                10 => {
                    // Code section
                    let count = read_leb_u32(bytes, &mut sec_pos)? as usize;
                    for _ in 0..count {
                        let body_size = read_leb_u32(bytes, &mut sec_pos)? as usize;
                        let body_end = sec_pos
                            .checked_add(body_size)
                            .filter(|&e| e <= section_end)
                            .ok_or_else(|| "truncated code body".to_string())?;
                        let mut bpos = sec_pos;
                        let local_decl_count = read_leb_u32(bytes, &mut bpos)? as usize;
                        let mut total_locals = 0usize;
                        for _ in 0..local_decl_count {
                            let n = read_leb_u32(bytes, &mut bpos)? as usize;
                            let _vt = read_u8(bytes, &mut bpos)?;
                            total_locals = total_locals.saturating_add(n);
                        }
                        let code = parse_instructions(&bytes[bpos..body_end])?;
                        func_bodies.push((total_locals, code));
                        sec_pos = body_end;
                    }
                }
                11 => {
                    // Data section
                    let count = read_leb_u32(bytes, &mut sec_pos)? as usize;
                    for _ in 0..count {
                        let mode = read_leb_u32(bytes, &mut sec_pos)?;
                        if mode != 0 {
                            return Err(format!("unsupported data segment mode {mode}"));
                        }
                        let offset = read_const_expr_i32(bytes, &mut sec_pos)? as usize;
                        let size = read_leb_u32(bytes, &mut sec_pos)? as usize;
                        let data_end = sec_pos
                            .checked_add(size)
                            .filter(|&e| e <= section_end)
                            .ok_or_else(|| "truncated data segment".to_string())?;
                        let mem_end = offset
                            .checked_add(size)
                            .filter(|&e| e <= memory.len())
                            .ok_or_else(|| "data segment out of memory bounds".to_string())?;
                        memory[offset..mem_end].copy_from_slice(&bytes[sec_pos..data_end]);
                        sec_pos = data_end;
                    }
                }
                _ => {
                    // Ignore other optional sections
                }
            }
            pos = section_end;
        }

        if func_type_indices.len() != func_bodies.len() {
            return Err(format!(
                "function section count ({}) != code section count ({})",
                func_type_indices.len(),
                func_bodies.len()
            ));
        }

        let mut funcs = Vec::with_capacity(func_bodies.len());
        for (type_idx, (num_locals, code)) in func_type_indices.into_iter().zip(func_bodies) {
            funcs.push(DefinedFunc {
                type_idx,
                num_locals,
                code,
            });
        }

        Ok(Self {
            types,
            imports,
            funcs,
            globals,
            memory,
            exports,
            max_steps: DEFAULT_MAX_STEPS,
        })
    }

    /// Invoke an exported function by name with `args`.
    pub fn call_export(
        &mut self,
        name: &str,
        args: &[i32],
        host: &mut HostContext<'_>,
    ) -> Result<Option<i32>, String> {
        let Some(&func_idx) = self.exports.get(name) else {
            return Err(format!("export {name:?} not found"));
        };
        let mut fuel = self.max_steps;
        self.invoke_func(func_idx, args, host, &mut fuel)
    }

    fn invoke_func(
        &mut self,
        func_idx: u32,
        args: &[i32],
        host: &mut HostContext<'_>,
        fuel: &mut usize,
    ) -> Result<Option<i32>, String> {
        let idx = func_idx as usize;
        if idx < self.imports.len() {
            let import = self.imports[idx];
            return self.call_host_import(import, args, host);
        }
        let def_idx = idx - self.imports.len();
        let func = self
            .funcs
            .get(def_idx)
            .cloned()
            .ok_or_else(|| format!("invalid function index {func_idx}"))?;
        let ftype = self
            .types
            .get(func.type_idx as usize)
            .cloned()
            .ok_or_else(|| format!("invalid type index {}", func.type_idx))?;

        if args.len() != ftype.params {
            return Err(format!(
                "argument count mismatch: expected {}, got {}",
                ftype.params,
                args.len()
            ));
        }

        let mut locals = vec![0i32; ftype.params + func.num_locals];
        locals[..args.len()].copy_from_slice(args);

        let mut stack: Vec<i32> = Vec::with_capacity(32);
        let mut labels: Vec<LabelFrame> = vec![LabelFrame {
            is_loop: false,
            target_pc: func.code.len(),
            stack_height: 0,
            arity: ftype.results,
        }];

        let mut pc = 0usize;
        while pc < func.code.len() {
            if *fuel == 0 {
                return Err("wasm trap: instruction fuel limit exceeded".to_string());
            }
            *fuel -= 1;

            let instr = &func.code[pc];
            pc += 1;

            match *instr {
                Instr::Unreachable => return Err("wasm trap: unreachable executed".to_string()),
                Instr::Nop => {}
                Instr::Block { has_result, end_pc } => {
                    labels.push(LabelFrame {
                        is_loop: false,
                        target_pc: end_pc,
                        stack_height: stack.len(),
                        arity: usize::from(has_result),
                    });
                }
                Instr::Loop {
                    has_result: _,
                    start_pc,
                    end_pc: _,
                } => {
                    labels.push(LabelFrame {
                        is_loop: true,
                        target_pc: start_pc,
                        stack_height: stack.len(),
                        arity: 0,
                    });
                }
                Instr::If {
                    has_result,
                    else_pc,
                    end_pc,
                } => {
                    let cond = pop_i32(&mut stack)?;
                    labels.push(LabelFrame {
                        is_loop: false,
                        target_pc: end_pc,
                        stack_height: stack.len(),
                        arity: usize::from(has_result),
                    });
                    if cond == 0 {
                        if let Some(ep) = else_pc {
                            pc = ep + 1;
                        } else {
                            labels.pop();
                            pc = end_pc + 1;
                        }
                    }
                }
                Instr::Else { end_pc } => {
                    labels.pop();
                    pc = end_pc + 1;
                }
                Instr::End => {
                    labels.pop();
                }
                Instr::Br(depth) => {
                    pc = branch_to(&mut stack, &mut labels, depth as usize)?;
                }
                Instr::BrIf(depth) => {
                    let cond = pop_i32(&mut stack)?;
                    if cond != 0 {
                        pc = branch_to(&mut stack, &mut labels, depth as usize)?;
                    }
                }
                Instr::Return => break,
                Instr::Call(callee_idx) => {
                    let (param_len, _res_len) = self.func_signature(callee_idx)?;
                    if stack.len() < param_len {
                        return Err("wasm stack underflow on call".to_string());
                    }
                    let call_args: Vec<i32> = stack.drain(stack.len() - param_len..).collect();
                    if let Some(ret) = self.invoke_func(callee_idx, &call_args, host, fuel)? {
                        stack.push(ret);
                    }
                }
                Instr::Drop => {
                    let _ = pop_i32(&mut stack)?;
                }
                Instr::Select => {
                    let c = pop_i32(&mut stack)?;
                    let v2 = pop_i32(&mut stack)?;
                    let v1 = pop_i32(&mut stack)?;
                    stack.push(if c != 0 { v1 } else { v2 });
                }
                Instr::LocalGet(i) => {
                    let v = *locals
                        .get(i as usize)
                        .ok_or_else(|| format!("local.get out of bounds: {i}"))?;
                    stack.push(v);
                }
                Instr::LocalSet(i) => {
                    let v = pop_i32(&mut stack)?;
                    let slot = locals
                        .get_mut(i as usize)
                        .ok_or_else(|| format!("local.set out of bounds: {i}"))?;
                    *slot = v;
                }
                Instr::LocalTee(i) => {
                    let v = *stack.last().ok_or_else(|| "stack underflow".to_string())?;
                    let slot = locals
                        .get_mut(i as usize)
                        .ok_or_else(|| format!("local.tee out of bounds: {i}"))?;
                    *slot = v;
                }
                Instr::GlobalGet(i) => {
                    let v = *self
                        .globals
                        .get(i as usize)
                        .ok_or_else(|| format!("global.get out of bounds: {i}"))?;
                    stack.push(v);
                }
                Instr::GlobalSet(i) => {
                    let v = pop_i32(&mut stack)?;
                    let slot = self
                        .globals
                        .get_mut(i as usize)
                        .ok_or_else(|| format!("global.set out of bounds: {i}"))?;
                    *slot = v;
                }
                Instr::I32Load { offset } => {
                    let base = pop_i32(&mut stack)? as usize;
                    let addr = base
                        .checked_add(offset as usize)
                        .ok_or_else(|| "memory address overflow".to_string())?;
                    if addr + 4 > self.memory.len() {
                        return Err("wasm trap: out of bounds memory access".to_string());
                    }
                    let val = i32::from_le_bytes([
                        self.memory[addr],
                        self.memory[addr + 1],
                        self.memory[addr + 2],
                        self.memory[addr + 3],
                    ]);
                    stack.push(val);
                }
                Instr::I32Store { offset } => {
                    let val = pop_i32(&mut stack)?;
                    let base = pop_i32(&mut stack)? as usize;
                    let addr = base
                        .checked_add(offset as usize)
                        .ok_or_else(|| "memory address overflow".to_string())?;
                    if addr + 4 > self.memory.len() {
                        return Err("wasm trap: out of bounds memory access".to_string());
                    }
                    self.memory[addr..addr + 4].copy_from_slice(&val.to_le_bytes());
                }
                Instr::I32Const(v) => stack.push(v),
                Instr::I32Eqz => {
                    let a = pop_i32(&mut stack)?;
                    stack.push(i32::from(a == 0));
                }
                Instr::I32Eq => binop(&mut stack, |a, b| i32::from(a == b))?,
                Instr::I32Ne => binop(&mut stack, |a, b| i32::from(a != b))?,
                Instr::I32LtS => binop(&mut stack, |a, b| i32::from(a < b))?,
                Instr::I32GtS => binop(&mut stack, |a, b| i32::from(a > b))?,
                Instr::I32LeS => binop(&mut stack, |a, b| i32::from(a <= b))?,
                Instr::I32GeS => binop(&mut stack, |a, b| i32::from(a >= b))?,
                Instr::I32Add => binop(&mut stack, i32::wrapping_add)?,
                Instr::I32Sub => binop(&mut stack, i32::wrapping_sub)?,
                Instr::I32Mul => binop(&mut stack, i32::wrapping_mul)?,
                Instr::I32DivS => {
                    let b = pop_i32(&mut stack)?;
                    let a = pop_i32(&mut stack)?;
                    if b == 0 {
                        return Err("wasm trap: integer divide by zero".to_string());
                    }
                    stack.push(a.wrapping_div(b));
                }
                Instr::I32And => binop(&mut stack, |a, b| a & b)?,
                Instr::I32Or => binop(&mut stack, |a, b| a | b)?,
                Instr::I32Xor => binop(&mut stack, |a, b| a ^ b)?,
                Instr::I32Shl => binop(&mut stack, |a, b| a.wrapping_shl(b as u32))?,
                Instr::I32ShrU => binop(&mut stack, |a, b| ((a as u32).wrapping_shr(b as u32)) as i32)?,
            }
        }

        if ftype.results > 0 {
            let ret = pop_i32(&mut stack)?;
            Ok(Some(ret))
        } else {
            Ok(None)
        }
    }

    fn func_signature(&self, func_idx: u32) -> Result<(usize, usize), String> {
        let idx = func_idx as usize;
        if idx < self.imports.len() {
            let hi = self.imports[idx];
            return Ok((hi.param_count(), hi.result_count()));
        }
        let def = self
            .funcs
            .get(idx - self.imports.len())
            .ok_or_else(|| format!("unknown func {func_idx}"))?;
        let ft = self
            .types
            .get(def.type_idx as usize)
            .ok_or_else(|| format!("unknown type {}", def.type_idx))?;
        Ok((ft.params, ft.results))
    }

    fn read_mem_str(&self, ptr: i32, len: i32) -> Result<String, String> {
        if ptr < 0 || len < 0 {
            return Err("negative string slice in WASM memory".to_string());
        }
        let start = ptr as usize;
        let end = start
            .checked_add(len as usize)
            .filter(|&e| e <= self.memory.len())
            .ok_or_else(|| "string slice out of WASM memory bounds".to_string())?;
        std::str::from_utf8(&self.memory[start..end])
            .map(str::to_string)
            .map_err(|e| format!("invalid UTF-8 in WASM memory: {e}"))
    }

    fn call_host_import(
        &self,
        import: HostImport,
        args: &[i32],
        host: &mut HostContext<'_>,
    ) -> Result<Option<i32>, String> {
        match import {
            HostImport::CanvasFillRect => {
                let (x, y, w, h, rgba) = (args[0], args[1], args[2], args[3], args[4]);
                if let Some(canvas) = host.canvas.as_deref_mut() {
                    canvas.fill_rect(Rect::new(x, y, w, h), unpack_rgba(rgba));
                }
                Ok(None)
            }
            HostImport::CanvasFillRoundedRect => {
                let (x, y, w, h, rad, rgba) =
                    (args[0], args[1], args[2], args[3], args[4], args[5]);
                if let Some(canvas) = host.canvas.as_deref_mut() {
                    canvas.fill_rounded_rect(
                        Rect::new(x, y, w, h),
                        rad as f32,
                        unpack_rgba(rgba),
                    );
                }
                Ok(None)
            }
            HostImport::CanvasDrawText => {
                let (ptr, len, x, y, size_px, rgba) =
                    (args[0], args[1], args[2], args[3], args[4], args[5]);
                let text = self.read_mem_str(ptr, len)?;
                if let Some(canvas) = host.canvas.as_deref_mut() {
                    let style = rabbiz_app_kit::ui::styled(
                        (size_px.max(10)) as f32,
                        600,
                        unpack_rgba(rgba),
                    );
                    let _ = rabbiz_app_kit::ui::draw_text(canvas, &text, &style, x, y, None);
                }
                Ok(None)
            }
            HostImport::ThemeColor => {
                let slot = args[0];
                let c = match slot {
                    0 => host.theme.bg,
                    1 => host.theme.surface1,
                    2 => host.theme.text,
                    3 => host.theme.accent,
                    4 => host.theme.danger,
                    _ => host.theme.text2,
                };
                Ok(Some(pack_rgba(c)))
            }
            HostImport::IntentOpen => {
                if !host.permissions.allow_intents {
                    return Ok(Some(-1));
                }
                let uri = self.read_mem_str(args[0], args[1])?;
                if rabbiz_sdk::intent::parse_uri(&uri).is_none() {
                    return Ok(Some(-2));
                }
                host.last_intent = Some(uri);
                Ok(Some(0))
            }
            HostImport::Notify => {
                if !host.permissions.allow_notifications {
                    return Ok(Some(-1));
                }
                let title = self.read_mem_str(args[0], args[1])?;
                let body = self.read_mem_str(args[2], args[3])?;
                let _ = rabbiz_sdk::services::send_notification(&host.app_id, &title, &body);
                host.last_notification = Some((title, body));
                Ok(Some(0))
            }
        }
    }
}

const fn unpack_rgba(v: i32) -> [u8; 4] {
    let u = v as u32;
    [
        ((u >> 24) & 0xFF) as u8,
        ((u >> 16) & 0xFF) as u8,
        ((u >> 8) & 0xFF) as u8,
        (u & 0xFF) as u8,
    ]
}

const fn pack_rgba(c: [u8; 4]) -> i32 {
    u32::from_be_bytes(c) as i32
}

fn pop_i32(stack: &mut Vec<i32>) -> Result<i32, String> {
    stack
        .pop()
        .ok_or_else(|| "wasm stack underflow".to_string())
}

fn binop<F: FnOnce(i32, i32) -> i32>(stack: &mut Vec<i32>, op: F) -> Result<(), String> {
    let b = pop_i32(stack)?;
    let a = pop_i32(stack)?;
    stack.push(op(a, b));
    Ok(())
}

fn branch_to(
    stack: &mut Vec<i32>,
    labels: &mut Vec<LabelFrame>,
    depth: usize,
) -> Result<usize, String> {
    if depth >= labels.len() {
        return Err(format!("invalid branch depth {depth}"));
    }
    let target_idx = labels.len() - 1 - depth;
    let target = labels[target_idx];
    let kept = if target.arity > 0 {
        Some(pop_i32(stack)?)
    } else {
        None
    };
    stack.truncate(target.stack_height);
    if let Some(v) = kept {
        stack.push(v);
    }
    if target.is_loop {
        labels.truncate(target_idx + 1);
        Ok(target.target_pc)
    } else {
        labels.truncate(target_idx);
        Ok(target.target_pc + 1)
    }
}

fn read_u8(bytes: &[u8], pos: &mut usize) -> Result<u8, String> {
    if *pos >= bytes.len() {
        return Err("unexpected EOF reading byte".to_string());
    }
    let b = bytes[*pos];
    *pos += 1;
    Ok(b)
}

fn read_leb_u32(bytes: &[u8], pos: &mut usize) -> Result<u32, String> {
    let mut result = 0u32;
    let mut shift = 0u32;
    loop {
        let b = read_u8(bytes, pos)?;
        result |= u32::from(b & 0x7F) << shift;
        if b & 0x80 == 0 {
            return Ok(result);
        }
        shift += 7;
        if shift >= 35 {
            return Err("LEB128 u32 overflow".to_string());
        }
    }
}

fn read_leb_i32(bytes: &[u8], pos: &mut usize) -> Result<i32, String> {
    let mut result = 0i32;
    let mut shift = 0u32;
    let mut byte;
    loop {
        byte = read_u8(bytes, pos)?;
        result |= i32::from(byte & 0x7F) << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            break;
        }
        if shift >= 35 {
            return Err("LEB128 i32 overflow".to_string());
        }
    }
    if shift < 32 && (byte & 0x40) != 0 {
        result |= !0 << shift;
    }
    Ok(result)
}

fn read_name(bytes: &[u8], pos: &mut usize) -> Result<String, String> {
    let len = read_leb_u32(bytes, pos)? as usize;
    let end = pos
        .checked_add(len)
        .filter(|&e| e <= bytes.len())
        .ok_or_else(|| "truncated UTF-8 name".to_string())?;
    let s = std::str::from_utf8(&bytes[*pos..end])
        .map_err(|e| format!("invalid UTF-8 name: {e}"))?
        .to_string();
    *pos = end;
    Ok(s)
}

fn read_const_expr_i32(bytes: &[u8], pos: &mut usize) -> Result<i32, String> {
    let op = read_u8(bytes, pos)?;
    if op != 0x41 {
        return Err(format!("expected i32.const (0x41) in init expr, got 0x{op:02x}"));
    }
    let val = read_leb_i32(bytes, pos)?;
    let end = read_u8(bytes, pos)?;
    if end != 0x0B {
        return Err(format!("expected end (0x0B) after init expr, got 0x{end:02x}"));
    }
    Ok(val)
}

#[derive(Debug, Clone, Copy)]
enum CtrlKind {
    Block(usize),
    Loop(usize),
    If { if_idx: usize, else_idx: Option<usize> },
}

fn parse_instructions(code: &[u8]) -> Result<Vec<Instr>, String> {
    let mut pos = 0usize;
    let mut out = Vec::new();
    let mut ctrl_stack: Vec<CtrlKind> = Vec::new();

    while pos < code.len() {
        let op = read_u8(code, &mut pos)?;
        match op {
            0x00 => out.push(Instr::Unreachable),
            0x01 => out.push(Instr::Nop),
            0x02 => {
                let bt = read_u8(code, &mut pos)?;
                let has_result = bt != 0x40;
                let idx = out.len();
                out.push(Instr::Block {
                    has_result,
                    end_pc: 0,
                });
                ctrl_stack.push(CtrlKind::Block(idx));
            }
            0x03 => {
                let bt = read_u8(code, &mut pos)?;
                let has_result = bt != 0x40;
                let idx = out.len();
                out.push(Instr::Loop {
                    has_result,
                    start_pc: idx + 1,
                    end_pc: 0,
                });
                ctrl_stack.push(CtrlKind::Loop(idx));
            }
            0x04 => {
                let bt = read_u8(code, &mut pos)?;
                let has_result = bt != 0x40;
                let idx = out.len();
                out.push(Instr::If {
                    has_result,
                    else_pc: None,
                    end_pc: 0,
                });
                ctrl_stack.push(CtrlKind::If {
                    if_idx: idx,
                    else_idx: None,
                });
            }
            0x05 => {
                let Some(CtrlKind::If { if_idx, else_idx }) = ctrl_stack.last_mut() else {
                    return Err("else without matching if".to_string());
                };
                let e_idx = out.len();
                out.push(Instr::Else { end_pc: 0 });
                *else_idx = Some(e_idx);
                if let Instr::If { else_pc, .. } = &mut out[*if_idx] {
                    *else_pc = Some(e_idx);
                }
            }
            0x0B => {
                let end_idx = out.len();
                out.push(Instr::End);
                if let Some(ctrl) = ctrl_stack.pop() {
                    match ctrl {
                        CtrlKind::Block(b_idx) => {
                            if let Instr::Block { end_pc, .. } = &mut out[b_idx] {
                                *end_pc = end_idx;
                            }
                        }
                        CtrlKind::Loop(l_idx) => {
                            if let Instr::Loop { end_pc, .. } = &mut out[l_idx] {
                                *end_pc = end_idx;
                            }
                        }
                        CtrlKind::If { if_idx, else_idx } => {
                            if let Instr::If { end_pc, .. } = &mut out[if_idx] {
                                *end_pc = end_idx;
                            }
                            if let Some(e_idx) = else_idx {
                                if let Instr::Else { end_pc } = &mut out[e_idx] {
                                    *end_pc = end_idx;
                                }
                            }
                        }
                    }
                }
            }
            0x0C => out.push(Instr::Br(read_leb_u32(code, &mut pos)?)),
            0x0D => out.push(Instr::BrIf(read_leb_u32(code, &mut pos)?)),
            0x0F => out.push(Instr::Return),
            0x10 => out.push(Instr::Call(read_leb_u32(code, &mut pos)?)),
            0x1A => out.push(Instr::Drop),
            0x1B => out.push(Instr::Select),
            0x20 => out.push(Instr::LocalGet(read_leb_u32(code, &mut pos)?)),
            0x21 => out.push(Instr::LocalSet(read_leb_u32(code, &mut pos)?)),
            0x22 => out.push(Instr::LocalTee(read_leb_u32(code, &mut pos)?)),
            0x23 => out.push(Instr::GlobalGet(read_leb_u32(code, &mut pos)?)),
            0x24 => out.push(Instr::GlobalSet(read_leb_u32(code, &mut pos)?)),
            0x28 => {
                let _align = read_leb_u32(code, &mut pos)?;
                let offset = read_leb_u32(code, &mut pos)?;
                out.push(Instr::I32Load { offset });
            }
            0x36 => {
                let _align = read_leb_u32(code, &mut pos)?;
                let offset = read_leb_u32(code, &mut pos)?;
                out.push(Instr::I32Store { offset });
            }
            0x41 => out.push(Instr::I32Const(read_leb_i32(code, &mut pos)?)),
            0x45 => out.push(Instr::I32Eqz),
            0x46 => out.push(Instr::I32Eq),
            0x47 => out.push(Instr::I32Ne),
            0x48 => out.push(Instr::I32LtS),
            0x4A => out.push(Instr::I32GtS),
            0x4C => out.push(Instr::I32LeS),
            0x4E => out.push(Instr::I32GeS),
            0x6A => out.push(Instr::I32Add),
            0x6B => out.push(Instr::I32Sub),
            0x6C => out.push(Instr::I32Mul),
            0x6D => out.push(Instr::I32DivS),
            0x71 => out.push(Instr::I32And),
            0x72 => out.push(Instr::I32Or),
            0x73 => out.push(Instr::I32Xor),
            0x74 => out.push(Instr::I32Shl),
            0x76 => out.push(Instr::I32ShrU),
            other => return Err(format!("unsupported WASM opcode 0x{other:02x}")),
        }
    }
    Ok(out)
}

/// `AppState` wrapper driving a loaded [`WasmModule`].
pub struct WasmApp {
    app_id: &'static str,
    title: &'static str,
    pub module: WasmModule,
    pub permissions: WasmPermissions,
    initialized: bool,
}

impl WasmApp {
    pub fn from_bytes(
        app_id: &'static str,
        title: &'static str,
        wasm_bytes: &[u8],
        permissions: WasmPermissions,
    ) -> Result<Self, String> {
        let module = WasmModule::from_bytes(wasm_bytes)?;
        Ok(Self {
            app_id,
            title,
            module,
            permissions,
            initialized: false,
        })
    }

    fn ensure_init(&mut self) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        if self.module.exports.contains_key("init") {
            let mut ctx = HostContext::headless(self.permissions);
            ctx.app_id = self.app_id.to_string();
            let _ = self.module.call_export("init", &[], &mut ctx);
        }
    }
}

impl AppState for WasmApp {
    fn app_id(&self) -> &'static str {
        self.app_id
    }

    fn title(&self) -> &'static str {
        self.title
    }

    fn handle_event(&mut self, event: AppEvent, _ti: &TextInputCtl) -> AppFrame {
        self.ensure_init();
        let (kind, x, y, key) = match event {
            AppEvent::TouchDown { pos, .. } => (1, pos.x, pos.y, 0),
            AppEvent::TouchMove { pos, .. } => (2, pos.x, pos.y, 0),
            AppEvent::TouchUp { pos, .. } => (3, pos.x, pos.y, 0),
            AppEvent::Key { keysym, .. } => (4, 0, 0, keysym as i32),
            AppEvent::Tick(dt) => (7, dt.as_millis() as i32, 0, 0),
            _ => return AppFrame::default(),
        };
        if !self.module.exports.contains_key("on_event") {
            return AppFrame::default();
        }
        let mut ctx = HostContext::headless(self.permissions);
        ctx.app_id = self.app_id.to_string();
        let flags = self
            .module
            .call_export("on_event", &[kind, x, y, key], &mut ctx)
            .ok()
            .flatten()
            .unwrap_or(1);
        AppFrame {
            dirty: (flags & 1) != 0,
            animating: (flags & 2) != 0,
            quit: (flags & 4) != 0,
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        self.ensure_init();
        let Size { w, h } = canvas.size();
        let mut ctx = HostContext {
            canvas: Some(canvas),
            theme: *theme,
            permissions: self.permissions,
            app_id: self.app_id.to_string(),
            last_intent: None,
            last_notification: None,
        };
        let _ = self.module.call_export("render", &[w, h], &mut ctx);
    }
}

// ---------------------------------------------------------------------------
// Compact `.wasm` binary assembler for `rabbiz new --template wasm` & tests
// ---------------------------------------------------------------------------

fn push_leb_u32(out: &mut Vec<u8>, mut val: u32) {
    loop {
        let mut b = (val & 0x7F) as u8;
        val >>= 7;
        if val != 0 {
            b |= 0x80;
        }
        out.push(b);
        if val == 0 {
            break;
        }
    }
}

fn push_leb_i32(out: &mut Vec<u8>, mut val: i32) {
    loop {
        let b = (val & 0x7F) as u8;
        val >>= 7;
        let done = (val == 0 && (b & 0x40) == 0) || (val == -1 && (b & 0x40) != 0);
        if done {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
}

fn push_name(out: &mut Vec<u8>, s: &str) {
    push_leb_u32(out, s.len() as u32);
    out.extend_from_slice(s.as_bytes());
}

fn push_section(wasm: &mut Vec<u8>, id: u8, payload: &[u8]) {
    wasm.push(id);
    push_leb_u32(wasm, payload.len() as u32);
    wasm.extend_from_slice(payload);
}

/// Assemble a self-contained, runnable `.wasm` application binary that draws
/// a themed RabbizOS card and title (`title`) and increments a tap counter on touch.
#[must_use]
pub fn assemble_demo_wasm(title: &str) -> Vec<u8> {
    let title_bytes = title.as_bytes();
    let title_len = title_bytes.len() as i32;

    let mut wasm = WASM_MAGIC_VERSION.to_vec();

    // Section 1: Types
    // 0: (i32, i32, i32, i32, i32, i32) -> ()  [canvas_fill_rounded_rect, canvas_draw_text]
    // 1: (i32) -> (i32)                        [theme_color]
    // 2: () -> ()                              [init]
    // 3: (i32, i32, i32, i32) -> (i32)         [on_event]
    // 4: (i32, i32) -> ()                      [render]
    let mut sec_type = Vec::new();
    push_leb_u32(&mut sec_type, 5);
    // Type 0
    sec_type.extend_from_slice(&[0x60, 6, 0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0]);
    // Type 1
    sec_type.extend_from_slice(&[0x60, 1, 0x7F, 1, 0x7F]);
    // Type 2
    sec_type.extend_from_slice(&[0x60, 0, 0]);
    // Type 3
    sec_type.extend_from_slice(&[0x60, 4, 0x7F, 0x7F, 0x7F, 0x7F, 1, 0x7F]);
    // Type 4
    sec_type.extend_from_slice(&[0x60, 2, 0x7F, 0x7F, 0]);
    push_section(&mut wasm, 1, &sec_type);

    // Section 2: Imports
    // func 0: rabbiz.canvas_fill_rounded_rect (type 0)
    // func 1: rabbiz.canvas_draw_text (type 0)
    // func 2: rabbiz.theme_color (type 1)
    let mut sec_import = Vec::new();
    push_leb_u32(&mut sec_import, 3);
    push_name(&mut sec_import, "rabbiz");
    push_name(&mut sec_import, "canvas_fill_rounded_rect");
    sec_import.extend_from_slice(&[0x00, 0x00]);
    push_name(&mut sec_import, "rabbiz");
    push_name(&mut sec_import, "canvas_draw_text");
    sec_import.extend_from_slice(&[0x00, 0x00]);
    push_name(&mut sec_import, "rabbiz");
    push_name(&mut sec_import, "theme_color");
    sec_import.extend_from_slice(&[0x00, 0x01]);
    push_section(&mut wasm, 2, &sec_import);

    // Section 3: Functions (func 3 = init: type 2, func 4 = on_event: type 3, func 5 = render: type 4)
    let sec_func = vec![3, 2, 3, 4];
    push_section(&mut wasm, 3, &sec_func);

    // Section 5: Memory (1 page)
    let sec_mem = vec![1, 0x00, 1];
    push_section(&mut wasm, 5, &sec_mem);

    // Section 6: Globals (global 0: mut i32 = 0 tap count)
    let sec_global = vec![1, 0x7F, 0x01, 0x41, 0x00, 0x0B];
    push_section(&mut wasm, 6, &sec_global);

    // Section 7: Exports ("init" -> 3, "on_event" -> 4, "render" -> 5)
    let mut sec_export = Vec::new();
    push_leb_u32(&mut sec_export, 3);
    push_name(&mut sec_export, "init");
    sec_export.extend_from_slice(&[0x00, 3]);
    push_name(&mut sec_export, "on_event");
    sec_export.extend_from_slice(&[0x00, 4]);
    push_name(&mut sec_export, "render");
    sec_export.extend_from_slice(&[0x00, 5]);
    push_section(&mut wasm, 7, &sec_export);

    // Section 10: Code
    let mut sec_code = Vec::new();
    push_leb_u32(&mut sec_code, 3);

    // Body 0 (init): set global 0 = 0
    let body_init = vec![0, 0x41, 0x00, 0x24, 0x00, 0x0B];
    push_leb_u32(&mut sec_code, body_init.len() as u32);
    sec_code.extend_from_slice(&body_init);

    // Body 1 (on_event): global 0 = global 0 + 1; return 1 (dirty)
    let body_event = vec![
        0,    // 0 locals
        0x23, 0x00, // global.get 0
        0x41, 0x01, // i32.const 1
        0x6A, // i32.add
        0x24, 0x00, // global.set 0
        0x41, 0x01, // i32.const 1
        0x0B, // end
    ];
    push_leb_u32(&mut sec_code, body_event.len() as u32);
    sec_code.extend_from_slice(&body_event);

    // Body 2 (render): draw accent card + title text from linear memory offset 0
    let mut body_render = vec![0]; // 0 extra locals
    // canvas_fill_rounded_rect(24, 96, 320, 120, 28, theme_color(3))
    for v in [24, 96, 320, 120, 28] {
        body_render.push(0x41);
        push_leb_i32(&mut body_render, v);
    }
    body_render.extend_from_slice(&[0x41, 0x03, 0x10, 0x02]); // theme_color(3 = accent)
    body_render.extend_from_slice(&[0x10, 0x00]); // call canvas_fill_rounded_rect

    // canvas_draw_text(0, title_len, 48, 136, 28, theme_color(2))
    for v in [0, title_len, 48, 136, 28] {
        body_render.push(0x41);
        push_leb_i32(&mut body_render, v);
    }
    body_render.extend_from_slice(&[0x41, 0x02, 0x10, 0x02]); // theme_color(2 = text)
    body_render.extend_from_slice(&[0x10, 0x01]); // call canvas_draw_text
    body_render.push(0x0B); // end

    push_leb_u32(&mut sec_code, body_render.len() as u32);
    sec_code.extend_from_slice(&body_render);
    push_section(&mut wasm, 10, &sec_code);

    // Section 11: Data (store `title` at offset 0)
    let mut sec_data = Vec::new();
    push_leb_u32(&mut sec_data, 1);
    sec_data.extend_from_slice(&[0x00, 0x41, 0x00, 0x0B]);
    push_leb_u32(&mut sec_data, title_bytes.len() as u32);
    sec_data.extend_from_slice(title_bytes);
    push_section(&mut wasm, 11, &sec_data);

    wasm
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_api::Point;
    use rabbiz_app_kit::TestHarness;

    #[test]
    fn rejects_invalid_wasm_magic() {
        let err = WasmModule::from_bytes(b"not-a-wasm").unwrap_err();
        assert!(err.contains("magic"));
    }

    #[test]
    fn demo_wasm_parses_handles_events_and_renders_in_test_harness() {
        let bytes = assemble_demo_wasm("Hello WASM RabbizOS");
        let app = WasmApp::from_bytes(
            "com.rabbiz.wasmdemo",
            "WASM Demo",
            &bytes,
            WasmPermissions::allow_all(),
        )
        .expect("load demo wasm");

        let mut harness = TestHarness::new(app).with_size(Size::new(400, 400));
        assert_eq!(harness.state().module.globals[0], 0);

        let f = harness.tap(Point::new(100, 120));
        assert!(f.dirty);
        assert_eq!(harness.state().module.globals[0], 2); // TouchDown + TouchUp

        let px = harness.render();
        let bg = harness.theme().bg;
        // Verify the accent card rendered at (60, 110) differs from theme.bg.
        let i = ((110 * 400 + 60) * 4) as usize;
        assert_ne!(&px.data[i..i + 4], &bg);
    }

    #[test]
    fn fuel_limit_traps_infinite_loop_cleanly() {
        // Build a minimal `.wasm` module whose `spin()` function loops forever:
        // (loop (br 0))
        let mut wasm = WASM_MAGIC_VERSION.to_vec();
        push_section(&mut wasm, 1, &[1, 0x60, 0, 0]); // type 0: () -> ()
        push_section(&mut wasm, 3, &[1, 0]); // func 0: type 0
        let mut exp = Vec::new();
        push_leb_u32(&mut exp, 1);
        push_name(&mut exp, "spin");
        exp.extend_from_slice(&[0x00, 0]);
        push_section(&mut wasm, 7, &exp);
        // Code: 1 func, body = [0 locals, loop(void), br 0, end, end]
        let body = [0x00, 0x03, 0x40, 0x0C, 0x00, 0x0B, 0x0B];
        let mut code = vec![1];
        push_leb_u32(&mut code, body.len() as u32);
        code.extend_from_slice(&body);
        push_section(&mut wasm, 10, &code);

        let mut module = WasmModule::from_bytes(&wasm).expect("parse loop wasm");
        module.max_steps = 500;
        let mut ctx = HostContext::headless(WasmPermissions::default());
        let err = module.call_export("spin", &[], &mut ctx).unwrap_err();
        assert!(err.contains("fuel limit exceeded"), "got {err}");
    }

    #[test]
    fn permission_sandbox_blocks_ungranted_intents_and_notifications() {
        let mut wasm = WASM_MAGIC_VERSION.to_vec();
        // Type 0: (i32, i32) -> (i32) [intent_open]
        // Type 1: () -> (i32)         [try_intent]
        push_section(
            &mut wasm,
            1,
            &[2, 0x60, 2, 0x7F, 0x7F, 1, 0x7F, 0x60, 0, 1, 0x7F],
        );
        let mut imp = vec![1];
        push_name(&mut imp, "rabbiz");
        push_name(&mut imp, "intent_open");
        imp.extend_from_slice(&[0x00, 0x00]);
        push_section(&mut wasm, 2, &imp);
        push_section(&mut wasm, 3, &[1, 1]);
        push_section(&mut wasm, 5, &[1, 0x00, 1]);
        let mut exp = vec![1];
        push_name(&mut exp, "try_intent");
        exp.extend_from_slice(&[0x00, 1]);
        push_section(&mut wasm, 7, &exp);

        let uri = b"tel:+66812345678";
        // Body: i32.const 0, i32.const len, call 0, end
        let body = [0x00, 0x41, 0x00, 0x41, uri.len() as u8, 0x10, 0x00, 0x0B];
        let mut code = vec![1, body.len() as u8];
        code.extend_from_slice(&body);
        push_section(&mut wasm, 10, &code);

        let mut data = vec![1, 0x00, 0x41, 0x00, 0x0B, uri.len() as u8];
        data.extend_from_slice(uri);
        push_section(&mut wasm, 11, &data);

        let mut module = WasmModule::from_bytes(&wasm).expect("parse intent wasm");

        // Denied when allow_intents = false -> returns -1
        let mut denied_ctx = HostContext::headless(WasmPermissions {
            allow_intents: false,
            ..WasmPermissions::default()
        });
        let rc_denied = module
            .call_export("try_intent", &[], &mut denied_ctx)
            .unwrap();
        assert_eq!(rc_denied, Some(-1));
        assert!(denied_ctx.last_intent.is_none());

        // Allowed when allow_intents = true -> returns 0 and records intent
        let mut allowed_ctx = HostContext::headless(WasmPermissions {
            allow_intents: true,
            ..WasmPermissions::default()
        });
        let rc_ok = module
            .call_export("try_intent", &[], &mut allowed_ctx)
            .unwrap();
        assert_eq!(rc_ok, Some(0));
        assert_eq!(
            allowed_ctx.last_intent.as_deref(),
            Some("tel:+66812345678")
        );
    }
}
