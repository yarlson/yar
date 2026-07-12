use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt::{self, Write},
};

use crate::{
    address_taken::collect_address_taken_locals,
    ast::*,
    checker::{
        CaptureInfo, EnumCaseInfo, EnumInfo, FunctionLiteralInfo, Info, InterfaceMethodInfo,
        Signature, const_integer_expression, function_literal_key, infer_untyped_integer_type,
        is_untyped_integer_expression,
    },
    token::Kind,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodegenError {
    message: String,
}

impl CodegenError {
    fn unsupported(feature: impl Into<String>) -> Self {
        Self {
            message: format!("unsupported Rust codegen feature: {}", feature.into()),
        }
    }
}

impl fmt::Display for CodegenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for CodegenError {}

#[cfg(test)]
fn emit_llvm(program: &Program, info: &Info) -> Result<String, CodegenError> {
    emit_llvm_with_options(program, info, None)
}

pub(crate) fn emit_llvm_with_options(
    program: &Program,
    info: &Info,
    target_triple: Option<&str>,
) -> Result<String, CodegenError> {
    let mut generator = Generator {
        info,
        target_triple,
        strings: BTreeMap::new(),
        literal_names: BTreeMap::new(),
        interface_globals: Vec::new(),
        interface_adapters: Vec::new(),
        interface_impls: BTreeMap::new(),
        task_wrappers: Vec::new(),
        next_string_id: 0,
        next_task_wrapper_id: 0,
    };
    generator.emit_program(program)
}

struct Generator<'a> {
    info: &'a Info,
    target_triple: Option<&'a str>,
    strings: BTreeMap<String, String>,
    literal_names: BTreeMap<String, String>,
    interface_globals: Vec<String>,
    interface_adapters: Vec<String>,
    interface_impls: BTreeMap<String, String>,
    task_wrappers: Vec<String>,
    next_string_id: usize,
    next_task_wrapper_id: usize,
}

impl Generator<'_> {
    fn emit_program(&mut self, program: &Program) -> Result<String, CodegenError> {
        let literals = collect_function_literals(program);
        for (idx, literal) in literals.iter().enumerate() {
            self.literal_names
                .insert(function_literal_key(literal), format!("closure.{idx}"));
        }

        let mut functions = Vec::new();
        for function in &program.functions {
            let signature = self
                .function_signature(function)
                .ok_or_else(|| {
                    CodegenError::unsupported(format!(
                        "missing signature {:?}",
                        function_signature_name(function)
                    ))
                })?
                .clone();
            functions.push(FunctionEmitter::new(self, function, &signature).emit()?);
        }
        let mut literal_functions = Vec::new();
        for literal in literals {
            let key = function_literal_key(literal);
            let name = self
                .literal_names
                .get(&key)
                .cloned()
                .ok_or_else(|| CodegenError::unsupported("missing function literal name"))?;
            let info = self
                .info
                .function_literals
                .get(&key)
                .ok_or_else(|| CodegenError::unsupported("missing function literal info"))?
                .clone();
            literal_functions
                .push(FunctionEmitter::new_literal(self, literal, &name, info).emit()?);
        }

        let main_wrapper = self.emit_main_wrapper()?;

        let mut out = String::new();
        out.push_str("; yar rust codegen\n");
        out.push_str("source_filename = \"yar\"\n");
        if let Some(target_triple) = self.target_triple {
            writeln!(&mut out, "target triple = {target_triple:?}").unwrap();
        }
        out.push('\n');
        out.push_str("%yar.str = type { ptr, i64 }\n");
        out.push_str("%yar.closure = type { ptr, ptr }\n");
        out.push_str("%yar.slice = type { ptr, i32, i32 }\n");
        out.push_str("%yar.iface = type { ptr, ptr }\n\n");
        for (name, info) in &self.info.interfaces {
            writeln!(
                &mut out,
                "{} = type {}",
                interface_table_type_name(name),
                interface_table_literal(info.methods.len())
            )
            .unwrap();
        }
        if !self.info.interfaces.is_empty() {
            out.push('\n');
        }
        for (name, info) in &self.info.structs {
            let fields = info
                .fields
                .iter()
                .map(|field| self.llvm_type(&field.type_))
                .collect::<Result<Vec<_>, _>>()?;
            writeln!(
                &mut out,
                "{} = type {{ {} }}",
                struct_type_name(name),
                fields.join(", ")
            )
            .unwrap();
        }
        for (name, info) in &self.info.enums {
            writeln!(
                &mut out,
                "{} = type {}",
                struct_type_name(name),
                self.enum_struct_literal(info)?
            )
            .unwrap();
        }
        if !self.info.structs.is_empty() || !self.info.enums.is_empty() {
            out.push('\n');
        }
        for result_type in self.result_types(program) {
            writeln!(
                &mut out,
                "{} = type {}",
                result_type_name(&result_type),
                result_struct_literal(&result_type)?
            )
            .unwrap();
        }
        if self.info.functions.values().any(|sig| sig.errorable) {
            out.push('\n');
        }
        out.push_str("declare void @yar_print(ptr, i64)\n");
        out.push_str("declare void @yar_panic(ptr, i64)\n");
        out.push_str("declare void @yar_eprint(ptr, i64)\n");
        out.push_str("declare ptr @yar_alloc(i64)\n");
        out.push_str("declare ptr @yar_alloc_zeroed(i64)\n");
        out.push_str("declare void @yar_pointer_check(ptr)\n");
        out.push_str("declare void @yar_i32_divrem_check(i32, i32)\n");
        out.push_str("declare void @yar_i64_divrem_check(i64, i64)\n");
        out.push_str("declare ptr @yar_map_new(i32, i32, i32)\n");
        out.push_str("declare void @yar_map_set(ptr, ptr, ptr)\n");
        out.push_str("declare i32 @yar_map_get(ptr, ptr, ptr)\n");
        out.push_str("declare i32 @yar_map_has(ptr, ptr)\n");
        out.push_str("declare void @yar_map_delete(ptr, ptr)\n");
        out.push_str("declare i32 @yar_map_len(ptr)\n");
        out.push_str("declare void @yar_map_keys(ptr, ptr)\n");
        out.push_str("declare void @yar_array_index_check(i64, i64)\n");
        out.push_str("declare void @yar_slice_index_check(i64, i64)\n");
        out.push_str("declare void @yar_slice_range_check(i64, i64, i64)\n");
        out.push_str("declare void @yar_str_index_check(i64, i64)\n");
        out.push_str("declare i32 @yar_str_equal(ptr, i64, ptr, i64)\n");
        out.push_str("declare void @yar_str_concat(ptr, i64, ptr, i64, ptr)\n");
        out.push_str("declare void @yar_str_from_byte(i32, ptr)\n");
        out.push_str("declare void @yar_to_str_i32(i32, ptr)\n");
        out.push_str("declare void @yar_to_str_i64(i64, ptr)\n");
        out.push_str("declare i64 @yar_sb_new()\n");
        out.push_str("declare void @yar_sb_write(i64, ptr, i64)\n");
        out.push_str("declare void @yar_sb_string(i64, ptr)\n");
        out.push_str("declare i32 @yar_fs_read_file(ptr, ptr)\n");
        out.push_str("declare i32 @yar_fs_write_file(ptr, ptr)\n");
        out.push_str("declare i32 @yar_fs_read_dir(ptr, ptr)\n");
        out.push_str("declare i32 @yar_fs_stat(ptr, ptr)\n");
        out.push_str("declare i32 @yar_fs_mkdir_all(ptr)\n");
        out.push_str("declare i32 @yar_fs_remove_all(ptr)\n");
        out.push_str("declare i32 @yar_fs_temp_dir(ptr, ptr)\n");
        out.push_str("declare i32 @yar_fs_open_read(ptr, ptr)\n");
        out.push_str("declare i32 @yar_fs_open_write(ptr, ptr)\n");
        out.push_str("declare i32 @yar_fs_read_handle(i64, i32, ptr)\n");
        out.push_str("declare i32 @yar_fs_write_handle(i64, ptr, ptr)\n");
        out.push_str("declare i32 @yar_fs_close_handle(i64)\n");
        out.push_str("declare i32 @yar_net_listen(ptr, i32, ptr)\n");
        out.push_str("declare i32 @yar_net_accept(i64, ptr)\n");
        out.push_str("declare i32 @yar_net_listener_addr(i64, ptr)\n");
        out.push_str("declare i32 @yar_net_close_listener(i64)\n");
        out.push_str("declare i32 @yar_net_connect(ptr, i32, ptr)\n");
        out.push_str("declare i32 @yar_net_read(i64, i32, ptr)\n");
        out.push_str("declare i32 @yar_net_write(i64, ptr, ptr)\n");
        out.push_str("declare i32 @yar_net_close(i64)\n");
        out.push_str("declare i32 @yar_net_local_addr(i64, ptr)\n");
        out.push_str("declare i32 @yar_net_remote_addr(i64, ptr)\n");
        out.push_str("declare i32 @yar_net_set_read_deadline(i64, i32)\n");
        out.push_str("declare i32 @yar_net_set_write_deadline(i64, i32)\n");
        out.push_str("declare i32 @yar_net_resolve(ptr, i32, ptr)\n");
        out.push_str("declare void @yar_process_args(ptr)\n");
        out.push_str("declare i32 @yar_process_run(ptr, i64, i64, i64, ptr, ptr)\n");
        out.push_str("declare i32 @yar_process_run_inherit(ptr, i64, ptr, ptr)\n");
        out.push_str("declare i32 @yar_env_lookup(ptr, ptr)\n");
        out.push_str("declare ptr @yar_taskgroup_new(i64)\n");
        out.push_str("declare void @yar_taskgroup_spawn(ptr, ptr, ptr)\n");
        out.push_str("declare void @yar_taskgroup_wait(ptr, ptr)\n");
        out.push_str("declare ptr @yar_chan_new(i64, i32)\n");
        out.push_str("declare i32 @yar_chan_send(ptr, ptr)\n");
        out.push_str("declare i32 @yar_chan_recv(ptr, ptr)\n");
        out.push_str("declare void @yar_chan_close(ptr)\n");
        out.push_str("declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)\n");
        out.push_str("declare void @yar_gc_init_stack_top(ptr)\n");
        out.push_str("declare void @yar_set_args(i32, ptr)\n\n");

        for global in &self.interface_globals {
            out.push_str(global);
        }
        if !self.interface_globals.is_empty() {
            out.push('\n');
        }

        for (value, name) in &self.strings {
            let bytes = escape_llvm_string(value);
            writeln!(
                &mut out,
                "@{name} = private unnamed_addr constant [{} x i8] c\"{}\\00\"",
                value.len() + 1,
                bytes
            )
            .unwrap();
        }
        if !self.strings.is_empty() {
            out.push('\n');
        }

        for function in functions {
            out.push_str(&function);
            out.push('\n');
        }
        for function in literal_functions {
            out.push_str(&function);
            out.push('\n');
        }
        for adapter in &self.interface_adapters {
            out.push_str(adapter);
            out.push('\n');
        }
        for wrapper in &self.task_wrappers {
            out.push_str(wrapper);
            out.push('\n');
        }
        out.push_str(&main_wrapper);
        Ok(out)
    }

    fn string_name(&mut self, value: &str) -> String {
        if let Some(name) = self.strings.get(value) {
            return name.clone();
        }
        let name = format!("yar.str.{}", self.next_string_id);
        self.next_string_id += 1;
        self.strings.insert(value.to_string(), name.clone());
        name
    }

    fn result_types(&self, program: &Program) -> Vec<String> {
        let mut result_types = self
            .info
            .functions
            .values()
            .filter(|sig| sig.errorable)
            .map(|sig| sig.return_type.clone())
            .collect::<Vec<_>>();
        result_types.extend(
            self.info
                .function_literals
                .values()
                .filter(|info| info.signature.errorable)
                .map(|info| info.signature.return_type.clone()),
        );
        collect_map_result_types(program, &mut result_types);
        result_types.sort();
        result_types.dedup();
        result_types
    }

    fn emit_main_wrapper(&mut self) -> Result<String, CodegenError> {
        let main = self
            .info
            .functions
            .get("main")
            .ok_or_else(|| CodegenError::unsupported("missing main function"))?;
        if main.errorable {
            return self.emit_errorable_main_wrapper(main);
        }
        if main.return_type != "i32" {
            return Err(CodegenError::unsupported(format!(
                "main return type {:?}",
                main.return_type
            )));
        }
        Ok(r#"define i32 @main(i32 %argc, ptr %argv) {
entry:
  %gc.stack.slot = alloca i8
  call void @yar_gc_init_stack_top(ptr %gc.stack.slot)
  call void @yar_set_args(i32 %argc, ptr %argv)
  %main.value = call i32 @yar.main()
  ret i32 %main.value
}
"#
        .to_string())
    }

    fn emit_errorable_main_wrapper(&mut self, main: &Signature) -> Result<String, CodegenError> {
        if main.return_type != "i32" {
            return Err(CodegenError::unsupported(format!(
                "errorable main return type {:?}",
                main.return_type
            )));
        }

        let result_type = result_type_name(&main.return_type);
        let mut out = String::new();
        writeln!(
            &mut out,
            r#"define i32 @main(i32 %argc, ptr %argv) {{
entry:
  %gc.stack.slot = alloca i8
  call void @yar_gc_init_stack_top(ptr %gc.stack.slot)
  call void @yar_set_args(i32 %argc, ptr %argv)
  %main.result = call {result_type} @yar.main()
  %main.is_err = extractvalue {result_type} %main.result, 0
  br i1 %main.is_err, label %main.err, label %main.ok

main.ok:
  %main.ok.value = extractvalue {result_type} %main.result, 2
  ret i32 %main.ok.value

main.err:
  %main.err.code = extractvalue {result_type} %main.result, 1"#
        )
        .unwrap();

        if self.info.ordered_errors.is_empty() {
            self.emit_print_literal(&mut out, "unhandled error\n", "main.err.message");
            out.push_str("  ret i32 1\n}\n");
            return Ok(out);
        }

        out.push_str("  switch i32 %main.err.code, label %main.err.unknown [\n");
        for name in &self.info.ordered_errors {
            let code = self.error_code(name)?;
            writeln!(&mut out, "    i32 {code}, label %main.err.{code}",).unwrap();
        }
        out.push_str("  ]\n\n");

        let errors = self.info.ordered_errors.clone();
        for name in errors {
            let code = self.error_code(&name)?;
            let display_name = self
                .info
                .error_display_names
                .get(&name)
                .ok_or_else(|| {
                    CodegenError::unsupported(format!("missing error display name {name:?}"))
                })?
                .clone();
            let label = code.to_string();
            writeln!(&mut out, "main.err.{label}:").unwrap();
            self.emit_print_literal(
                &mut out,
                "unhandled error: ",
                &format!("main.err.prefix.{label}"),
            );
            self.emit_print_literal(&mut out, &display_name, &format!("main.err.name.{label}"));
            self.emit_print_literal(&mut out, "\n", &format!("main.err.newline.{label}"));
            out.push_str("  ret i32 1\n\n");
        }

        out.push_str("main.err.unknown:\n");
        self.emit_print_literal(&mut out, "unhandled error\n", "main.err.unknown.message");
        out.push_str("  ret i32 1\n}\n");
        Ok(out)
    }

    fn emit_print_literal(&mut self, out: &mut String, value: &str, local: &str) {
        let global = self.string_name(value);
        writeln!(
            out,
            "  %{local} = getelementptr inbounds [{} x i8], ptr @{global}, i64 0, i64 0",
            value.len() + 1
        )
        .unwrap();
        writeln!(
            out,
            "  call void @yar_print(ptr %{local}, i64 {})",
            value.len()
        )
        .unwrap();
    }

    fn error_code(&self, name: &str) -> Result<i32, CodegenError> {
        self.info
            .error_codes
            .get(name)
            .copied()
            .ok_or_else(|| CodegenError::unsupported(format!("missing error code {name:?}")))
    }

    fn struct_field(&self, type_: &str, field_name: &str) -> Result<(usize, String), CodegenError> {
        let info = self
            .info
            .structs
            .get(type_)
            .ok_or_else(|| CodegenError::unsupported(format!("unknown struct {type_:?}")))?;
        info.fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name == field_name)
            .map(|(idx, field)| (idx, field.type_.clone()))
            .ok_or_else(|| {
                CodegenError::unsupported(format!("unknown field {type_:?}.{field_name}"))
            })
    }

    fn function_signature(&self, function: &FunctionDecl) -> Option<&Signature> {
        if let Some(receiver) = &function.receiver {
            return self
                .info
                .methods
                .get(&receiver.type_ref.to_string())
                .and_then(|methods| methods.get(&function.name));
        }
        self.info.functions.get(&function.name)
    }

    fn method_signature(&self, receiver: &str, name: &str) -> Option<Signature> {
        self.info
            .methods
            .get(receiver)
            .and_then(|methods| methods.get(name))
            .cloned()
    }

    fn ensure_interface_impl(
        &mut self,
        interface_type: &str,
        concrete_type: &str,
    ) -> Result<String, CodegenError> {
        let key = format!("{interface_type}=>{concrete_type}");
        if let Some(name) = self.interface_impls.get(&key) {
            return Ok(format!("@{name}"));
        }

        let interface = self
            .info
            .interfaces
            .get(interface_type)
            .cloned()
            .ok_or_else(|| {
                CodegenError::unsupported(format!("unknown interface {interface_type:?}"))
            })?;

        let mut fields = Vec::new();
        for method in &interface.methods {
            let signature = self
                .method_signature(concrete_type, &method.name)
                .ok_or_else(|| {
                    CodegenError::unsupported(format!(
                        "missing interface method {concrete_type}.{}",
                        method.name
                    ))
                })?;
            let adapter_name = interface_adapter_name(interface_type, concrete_type, &method.name);
            let adapter = self.emit_interface_adapter(&adapter_name, &signature)?;
            self.interface_adapters.push(adapter);
            fields.push(format!("ptr @{}", function_symbol(&adapter_name)));
        }

        let global_name = interface_table_global_name(interface_type, concrete_type);
        let literal = if fields.is_empty() {
            "{ }".to_string()
        } else {
            format!("{{ {} }}", fields.join(", "))
        };
        self.interface_globals.push(format!(
            "@{global_name} = private unnamed_addr constant {} {literal}\n",
            interface_table_type_name(interface_type)
        ));
        self.interface_impls.insert(key, global_name.clone());
        Ok(format!("@{global_name}"))
    }

    fn emit_interface_adapter(
        &self,
        name: &str,
        signature: &Signature,
    ) -> Result<String, CodegenError> {
        let return_type = if signature.errorable {
            result_type_name(&signature.return_type)
        } else {
            self.llvm_type(&signature.return_type)?
        };

        let mut params = vec!["ptr %data".to_string()];
        for (idx, param) in signature.params.iter().skip(1).enumerate() {
            params.push(format!("{} %arg{idx}", self.llvm_type(param)?));
        }

        let mut out = String::new();
        writeln!(
            &mut out,
            "define {return_type} @{}({}) {{",
            function_symbol(name),
            params.join(", ")
        )
        .unwrap();
        out.push_str("entry:\n");

        let mut call_args = Vec::new();
        if parse_pointer_type(&signature.receiver).is_some() {
            call_args.push("ptr %data".to_string());
        } else {
            let receiver_type = self.llvm_type(&signature.receiver)?;
            writeln!(&mut out, "  %recv = load {receiver_type}, ptr %data").unwrap();
            call_args.push(format!("{receiver_type} %recv"));
        }
        for (idx, param) in signature.params.iter().skip(1).enumerate() {
            call_args.push(format!("{} %arg{idx}", self.llvm_type(param)?));
        }

        if !signature.errorable && matches!(signature.return_type.as_str(), "void" | "noreturn") {
            writeln!(
                &mut out,
                "  call void @{}({})",
                function_symbol(&signature.full_name),
                call_args.join(", ")
            )
            .unwrap();
            if signature.return_type == "noreturn" {
                out.push_str("  unreachable\n");
            } else {
                out.push_str("  ret void\n");
            }
            out.push_str("}\n");
            return Ok(out);
        }

        writeln!(
            &mut out,
            "  %call = call {return_type} @{}({})",
            function_symbol(&signature.full_name),
            call_args.join(", ")
        )
        .unwrap();
        writeln!(&mut out, "  ret {return_type} %call").unwrap();
        out.push_str("}\n");
        Ok(out)
    }

    fn queue_task_function_wrapper(
        &mut self,
        signature: &Signature,
    ) -> Result<(String, String), CodegenError> {
        if signature.method || signature.builtin {
            return Err(CodegenError::unsupported(format!(
                "spawn call target {:?}",
                signature.full_name
            )));
        }

        let name = format!("task.wrapper.{}", self.next_task_wrapper_id);
        self.next_task_wrapper_id += 1;
        let ctx_type = self.task_wrapper_context_type(&signature.params)?;
        let wrapper = self.emit_task_function_wrapper(&name, signature, &ctx_type)?;
        self.task_wrappers.push(wrapper);
        Ok((name, ctx_type))
    }

    fn queue_task_closure_wrapper(
        &mut self,
        function_type: &FunctionType,
    ) -> Result<(String, String), CodegenError> {
        let name = format!("task.wrapper.{}", self.next_task_wrapper_id);
        self.next_task_wrapper_id += 1;
        let mut fields = vec![format_function_type(
            &function_type.params,
            &function_type.return_type,
            function_type.errorable,
        )];
        fields.extend(function_type.params.clone());
        let ctx_type = self.task_wrapper_context_type(&fields)?;
        let wrapper = self.emit_task_closure_wrapper(&name, function_type, &ctx_type)?;
        self.task_wrappers.push(wrapper);
        Ok((name, ctx_type))
    }

    fn task_wrapper_context_type(&self, fields: &[String]) -> Result<String, CodegenError> {
        if fields.is_empty() {
            return Ok("{ }".to_string());
        }
        let fields = fields
            .iter()
            .map(|param| self.llvm_type(param))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(format!("{{ {} }}", fields.join(", ")))
    }

    fn task_wrapper_context_size(&self, fields: &[String]) -> Result<i32, CodegenError> {
        let mut size = 0_i32;
        let mut align = 1_i32;
        for param in fields {
            let field_align = self.type_align(param)?;
            size = align_to(size, field_align)?;
            size = size
                .checked_add(self.type_size(param)?)
                .ok_or_else(|| CodegenError::unsupported("task wrapper context size overflow"))?;
            align = align.max(field_align);
        }
        align_to(size, align)
    }

    fn emit_task_function_wrapper(
        &self,
        name: &str,
        signature: &Signature,
        ctx_type: &str,
    ) -> Result<String, CodegenError> {
        let mut out = String::new();
        writeln!(
            &mut out,
            "define void @{}(ptr %ctx, ptr %result) {{",
            function_symbol(name)
        )
        .unwrap();
        out.push_str("entry:\n");

        let mut args = Vec::new();
        for (idx, param) in signature.params.iter().enumerate() {
            let llvm_type = self.llvm_type(param)?;
            writeln!(
                &mut out,
                "  %arg.ptr.{idx} = getelementptr inbounds {ctx_type}, ptr %ctx, i32 0, i32 {idx}"
            )
            .unwrap();
            writeln!(
                &mut out,
                "  %arg.{idx} = load {llvm_type}, ptr %arg.ptr.{idx}"
            )
            .unwrap();
            args.push(format!("{llvm_type} %arg.{idx}"));
        }

        if signature.host_intrinsic && signature.full_name == "fs.read_file" {
            self.emit_task_fs_read_file_wrapper_result(&mut out)?;
            out.push_str("}\n");
            return Ok(out);
        }

        if !signature.errorable && matches!(signature.return_type.as_str(), "void" | "noreturn") {
            writeln!(
                &mut out,
                "  call void @{}({})",
                function_symbol(&signature.full_name),
                args.join(", ")
            )
            .unwrap();
            if signature.return_type == "noreturn" {
                out.push_str("  unreachable\n");
            } else {
                out.push_str("  ret void\n");
            }
            out.push_str("}\n");
            return Ok(out);
        }

        let return_type = if signature.errorable {
            result_type_name(&signature.return_type)
        } else {
            self.llvm_type(&signature.return_type)?
        };
        writeln!(
            &mut out,
            "  %call = call {return_type} @{}({})",
            function_symbol(&signature.full_name),
            args.join(", ")
        )
        .unwrap();
        writeln!(&mut out, "  store {return_type} %call, ptr %result").unwrap();
        out.push_str("  ret void\n");
        out.push_str("}\n");
        Ok(out)
    }

    fn emit_task_fs_read_file_wrapper_result(&self, out: &mut String) -> Result<(), CodegenError> {
        out.push_str("  %fs.read_file.arg = alloca %yar.str\n");
        out.push_str("  store %yar.str %arg.0, ptr %fs.read_file.arg\n");
        out.push_str("  %fs.read_file.out = alloca %yar.str\n");
        out.push_str("  store %yar.str zeroinitializer, ptr %fs.read_file.out\n");
        out.push_str(
            "  %fs.read_file.status = call i32 @yar_fs_read_file(ptr %fs.read_file.arg, ptr %fs.read_file.out)\n",
        );
        out.push_str("  %fs.read_file.value = load %yar.str, ptr %fs.read_file.out\n");
        out.push_str("  %host.ok = icmp eq i32 %fs.read_file.status, 0\n");
        out.push_str("  br i1 %host.ok, label %host.ok.block, label %host.err.block\n\n");
        out.push_str("host.ok.block:\n");
        out.push_str(
            "  %host.ok.result.0 = insertvalue %yar.result.str zeroinitializer, i1 false, 0\n",
        );
        out.push_str(
            "  %host.ok.result.1 = insertvalue %yar.result.str %host.ok.result.0, i32 0, 1\n",
        );
        out.push_str(
            "  %host.ok.result.2 = insertvalue %yar.result.str %host.ok.result.1, %yar.str %fs.read_file.value, 2\n",
        );
        out.push_str("  br label %host.end\n\n");
        out.push_str("host.err.block:\n");
        let err_code = self.emit_task_fs_error_code(out, "%fs.read_file.status")?;
        out.push_str(
            "  %host.err.result.0 = insertvalue %yar.result.str zeroinitializer, i1 true, 0\n",
        );
        writeln!(
            out,
            "  %host.err.result.1 = insertvalue %yar.result.str %host.err.result.0, i32 {err_code}, 1"
        )
        .unwrap();
        out.push_str(
            "  %host.err.result.2 = insertvalue %yar.result.str %host.err.result.1, %yar.str zeroinitializer, 2\n",
        );
        out.push_str("  br label %host.end\n\n");
        out.push_str("host.end:\n");
        out.push_str(
            "  %host.result = phi %yar.result.str [%host.ok.result.2, %host.ok.block], [%host.err.result.2, %host.err.block]\n",
        );
        out.push_str("  store %yar.result.str %host.result, ptr %result\n");
        out.push_str("  ret void\n");
        Ok(())
    }

    fn emit_task_fs_error_code(
        &self,
        out: &mut String,
        status: &str,
    ) -> Result<String, CodegenError> {
        let mut code = self.error_code("fs.IO")?.to_string();
        for (idx, (status_code, name)) in [
            (7, "Closed"),
            (6, "InvalidArgument"),
            (4, "InvalidPath"),
            (3, "AlreadyExists"),
            (2, "PermissionDenied"),
            (1, "NotFound"),
        ]
        .into_iter()
        .enumerate()
        {
            let identity = host_error_identity("fs", name);
            writeln!(
                out,
                "  %host.err.match.{idx} = icmp eq i32 {status}, {status_code}"
            )
            .unwrap();
            writeln!(
                out,
                "  %host.err.code.{idx} = select i1 %host.err.match.{idx}, i32 {}, i32 {code}",
                self.error_code(&identity)?
            )
            .unwrap();
            code = format!("%host.err.code.{idx}");
        }
        Ok(code)
    }

    fn emit_task_closure_wrapper(
        &self,
        name: &str,
        function_type: &FunctionType,
        ctx_type: &str,
    ) -> Result<String, CodegenError> {
        let mut out = String::new();
        writeln!(
            &mut out,
            "define void @{}(ptr %ctx, ptr %result) {{",
            function_symbol(name)
        )
        .unwrap();
        out.push_str("entry:\n");
        writeln!(
            &mut out,
            "  %callee.ptr = getelementptr inbounds {ctx_type}, ptr %ctx, i32 0, i32 0"
        )
        .unwrap();
        writeln!(&mut out, "  %callee = load %yar.closure, ptr %callee.ptr").unwrap();
        writeln!(
            &mut out,
            "  %callee.code = extractvalue %yar.closure %callee, 0"
        )
        .unwrap();
        writeln!(
            &mut out,
            "  %callee.env = extractvalue %yar.closure %callee, 1"
        )
        .unwrap();

        let mut args = vec!["ptr %callee.env".to_string()];
        for (idx, param) in function_type.params.iter().enumerate() {
            let field_idx = idx + 1;
            let llvm_type = self.llvm_type(param)?;
            writeln!(
                &mut out,
                "  %arg.ptr.{idx} = getelementptr inbounds {ctx_type}, ptr %ctx, i32 0, i32 {field_idx}"
            )
            .unwrap();
            writeln!(
                &mut out,
                "  %arg.{idx} = load {llvm_type}, ptr %arg.ptr.{idx}"
            )
            .unwrap();
            args.push(format!("{llvm_type} %arg.{idx}"));
        }

        let return_type = if function_type.errorable {
            result_type_name(&function_type.return_type)
        } else {
            self.llvm_type(&function_type.return_type)?
        };
        if !function_type.errorable && function_type.return_type == "void" {
            writeln!(&mut out, "  call void %callee.code({})", args.join(", ")).unwrap();
            out.push_str("  ret void\n");
            out.push_str("}\n");
            return Ok(out);
        }

        writeln!(
            &mut out,
            "  %call = call {return_type} %callee.code({})",
            args.join(", ")
        )
        .unwrap();
        writeln!(&mut out, "  store {return_type} %call, ptr %result").unwrap();
        out.push_str("  ret void\n");
        out.push_str("}\n");
        Ok(out)
    }

    fn enum_case(&self, enum_type: &str, case_name: &str) -> Result<EnumCaseInfo, CodegenError> {
        let enum_info = self
            .info
            .enums
            .get(enum_type)
            .ok_or_else(|| CodegenError::unsupported(format!("unknown enum {enum_type:?}")))?;
        enum_info
            .cases
            .iter()
            .find(|case| case.name == case_name)
            .cloned()
            .ok_or_else(|| {
                CodegenError::unsupported(format!("unknown enum case {enum_type:?}.{case_name}"))
            })
    }

    fn enum_case_type(&self, type_: &str) -> Result<Option<(String, EnumCaseInfo)>, CodegenError> {
        let Some((enum_type, case_name)) = type_.rsplit_once('.') else {
            return Ok(None);
        };
        if !self.info.enums.contains_key(enum_type) {
            return Ok(None);
        }
        Ok(Some((
            enum_type.to_string(),
            self.enum_case(enum_type, case_name)?,
        )))
    }

    fn enum_payload_words(&self, info: &EnumInfo) -> Result<i32, CodegenError> {
        let mut max_size = 0_i32;
        for case in &info.cases {
            if case.payload_type.is_empty() {
                continue;
            }
            max_size = max_size.max(self.type_size(&case.payload_type)?);
        }
        if max_size == 0 {
            return Ok(0);
        }
        Ok((max_size + 7) / 8)
    }

    fn enum_struct_literal(&self, info: &EnumInfo) -> Result<String, CodegenError> {
        let payload_words = self.enum_payload_words(info)?;
        if payload_words == 0 {
            return Ok("{ i32 }".to_string());
        }
        Ok(format!("{{ i32, [{payload_words} x i64] }}"))
    }

    fn type_size(&self, type_: &str) -> Result<i32, CodegenError> {
        if let Some(inner) = type_.strip_prefix('!') {
            if inner == "void" {
                return Ok(8);
            }
            return Ok(8 + self.type_size(inner)?);
        }
        match type_ {
            "bool" => return Ok(1),
            "i32" | "error" => return Ok(4),
            "i64" => return Ok(8),
            "str" => return Ok(16),
            _ => {}
        }
        if parse_slice_type(type_).is_some() {
            return Ok(16);
        }
        if parse_function_type(type_).is_some() {
            return Ok(16);
        }
        if parse_pointer_type(type_).is_some() {
            return Ok(8);
        }
        if parse_map_type(type_).is_some() {
            return Ok(8);
        }
        if parse_chan_type(type_).is_some() {
            return Ok(8);
        }
        if self.info.interfaces.contains_key(type_) {
            return Ok(16);
        }
        if let Some((len, element_type)) = parse_array_type(type_) {
            let len = i32::try_from(len).map_err(|_| CodegenError::unsupported("array size"))?;
            return len
                .checked_mul(self.type_size(&element_type)?)
                .ok_or_else(|| CodegenError::unsupported("array size overflow"));
        }
        if let Some(info) = self.info.structs.get(type_) {
            let mut size = 0_i32;
            let mut align = 1_i32;
            for field in &info.fields {
                let field_align = self.type_align(&field.type_)?;
                size = align_to(size, field_align)?;
                size = size
                    .checked_add(self.type_size(&field.type_)?)
                    .ok_or_else(|| CodegenError::unsupported("struct size overflow"))?;
                align = align.max(field_align);
            }
            return align_to(size, align);
        }
        if let Some(info) = self.info.enums.get(type_) {
            let payload_words = self.enum_payload_words(info)?;
            if payload_words == 0 {
                return Ok(4);
            }
            return Ok(8 + payload_words * 8);
        }
        Err(CodegenError::unsupported(format!("size of type {type_:?}")))
    }

    fn type_align(&self, type_: &str) -> Result<i32, CodegenError> {
        if let Some(inner) = type_.strip_prefix('!') {
            if inner == "void" {
                return Ok(4);
            }
            return Ok(self.type_align(inner)?.max(4));
        }
        match type_ {
            "bool" => return Ok(1),
            "i32" | "error" => return Ok(4),
            "i64" | "str" => return Ok(8),
            _ => {}
        }
        if parse_slice_type(type_).is_some()
            || parse_function_type(type_).is_some()
            || parse_pointer_type(type_).is_some()
            || parse_map_type(type_).is_some()
            || parse_chan_type(type_).is_some()
            || self.info.interfaces.contains_key(type_)
        {
            return Ok(8);
        }
        if let Some((_, element_type)) = parse_array_type(type_) {
            return self.type_align(&element_type);
        }
        if let Some(info) = self.info.structs.get(type_) {
            return info
                .fields
                .iter()
                .map(|field| self.type_align(&field.type_))
                .try_fold(1_i32, |align, field_align| Ok(align.max(field_align?)));
        }
        if let Some(info) = self.info.enums.get(type_) {
            if self.enum_payload_words(info)? == 0 {
                return Ok(4);
            }
            return Ok(8);
        }
        Err(CodegenError::unsupported(format!(
            "align of type {type_:?}"
        )))
    }

    fn llvm_type(&self, type_: &str) -> Result<String, CodegenError> {
        if let Some(inner) = type_.strip_prefix('!') {
            return Ok(result_type_name(inner));
        }
        match type_ {
            "void" => Ok("void".to_string()),
            "noreturn" => Ok("void".to_string()),
            "bool" => Ok("i1".to_string()),
            "i32" => Ok("i32".to_string()),
            "i64" => Ok("i64".to_string()),
            "str" => Ok("%yar.str".to_string()),
            "error" => Ok("i32".to_string()),
            other => {
                if parse_pointer_type(other).is_some() {
                    return Ok("ptr".to_string());
                }
                if parse_function_type(other).is_some() {
                    return Ok("%yar.closure".to_string());
                }
                if parse_map_type(other).is_some() {
                    return Ok("ptr".to_string());
                }
                if parse_chan_type(other).is_some() {
                    return Ok("ptr".to_string());
                }
                if parse_slice_type(other).is_some() {
                    return Ok("%yar.slice".to_string());
                }
                if self.info.interfaces.contains_key(other) {
                    return Ok("%yar.iface".to_string());
                }
                if let Some((len, elem)) = parse_array_type(other) {
                    return Ok(format!("[{len} x {}]", self.llvm_type(&elem)?));
                }
                Ok(struct_type_name(other))
            }
        }
    }
}

struct FunctionEmitter<'a, 'g> {
    generator: &'a mut Generator<'g>,
    symbol_name: String,
    params: Vec<FunctionParam>,
    body_block: &'a BlockStmt,
    signature: Signature,
    captures: Vec<CaptureInfo>,
    heap_locals: BTreeSet<String>,
    locals: BTreeMap<String, Local>,
    loop_stack: Vec<LoopLabels>,
    taskgroup: Option<TaskgroupContext>,
    next_id: usize,
    body: String,
    terminated: bool,
    current_block: String,
}

impl<'a, 'g> FunctionEmitter<'a, 'g> {
    fn new(
        generator: &'a mut Generator<'g>,
        function: &'a FunctionDecl,
        signature: &'a Signature,
    ) -> Self {
        let mut params = Vec::new();
        let param_offset = if let Some(receiver) = &function.receiver {
            params.push(FunctionParam {
                name: receiver.name.clone(),
                type_: signature.params[0].clone(),
            });
            1
        } else {
            0
        };
        params.extend(
            function
                .params
                .iter()
                .enumerate()
                .map(|(idx, param)| FunctionParam {
                    name: param.name.clone(),
                    type_: signature.params[idx + param_offset].clone(),
                }),
        );
        Self {
            generator,
            symbol_name: signature.full_name.clone(),
            params,
            body_block: &function.body,
            signature: signature.clone(),
            captures: Vec::new(),
            heap_locals: collect_address_taken_locals(&function.body),
            locals: BTreeMap::new(),
            loop_stack: Vec::new(),
            taskgroup: None,
            next_id: 0,
            body: String::new(),
            terminated: false,
            current_block: "entry".to_string(),
        }
    }

    fn new_literal(
        generator: &'a mut Generator<'g>,
        literal: &'a FunctionLiteralExpr,
        name: &str,
        info: FunctionLiteralInfo,
    ) -> Self {
        let params = literal
            .params
            .iter()
            .enumerate()
            .map(|(idx, param)| FunctionParam {
                name: param.name.clone(),
                type_: info.signature.params[idx].clone(),
            })
            .collect();
        Self {
            generator,
            symbol_name: name.to_string(),
            params,
            body_block: &literal.body,
            signature: info.signature,
            captures: info.captures,
            heap_locals: collect_address_taken_locals(&literal.body),
            locals: BTreeMap::new(),
            loop_stack: Vec::new(),
            taskgroup: None,
            next_id: 0,
            body: String::new(),
            terminated: false,
            current_block: "entry".to_string(),
        }
    }

    fn emit(mut self) -> Result<String, CodegenError> {
        let mut params = Vec::new();
        if !self.captures.is_empty() || self.symbol_name.starts_with("closure.") {
            params.push("ptr %env".to_string());
        }
        if !self.captures.is_empty() {
            let env_type = self.closure_env_type_literal(&self.captures)?;
            for (idx, capture) in self.captures.clone().iter().enumerate() {
                let ptr = self.temp("capture.ptr");
                self.body.push_str(&format!(
                    "  %{ptr} = getelementptr inbounds {env_type}, ptr %env, i32 0, i32 {idx}\n"
                ));
                self.locals.insert(
                    capture.name.clone(),
                    Local {
                        type_: capture.type_.clone(),
                        ptr: format!("%{ptr}"),
                    },
                );
            }
        }
        for param in self.params.clone() {
            let type_ = param.type_;
            let llvm_type = self.llvm_type(&type_)?;
            let register = format!("%arg.{}", param.name);
            params.push(format!("{llvm_type} {register}"));
            let slot = self.allocate_local_slot(&param.name, &type_)?;
            self.body
                .push_str(&format!("  store {llvm_type} {register}, ptr {slot}\n"));
            self.locals.insert(param.name, Local { type_, ptr: slot });
        }

        for statement in &self.body_block.stmts {
            self.emit_statement(statement)?;
            if self.terminated {
                break;
            }
        }
        if !self.terminated {
            self.emit_fallthrough_return()?;
        }

        let mut out = String::new();
        let return_type = if self.signature.errorable {
            result_type_name(&self.signature.return_type)
        } else {
            self.llvm_type(&self.signature.return_type)?.to_string()
        };
        writeln!(
            &mut out,
            "define {} @{}({}) {{",
            return_type,
            function_symbol(&self.symbol_name),
            params.join(", ")
        )
        .unwrap();
        out.push_str("entry:\n");
        out.push_str(&self.body);
        out.push_str("}\n");
        Ok(out)
    }

    fn emit_statement(&mut self, statement: &Statement) -> Result<(), CodegenError> {
        match statement {
            Statement::Let(stmt) => {
                let value = match infer_untyped_integer_type(&stmt.value) {
                    Some(type_) => self.emit_expression_as(&stmt.value, type_)?,
                    None => self.emit_expression(&stmt.value)?,
                };
                self.bind_local(&stmt.name, value)?;
                Ok(())
            }
            Statement::Var(stmt) => self.emit_var(stmt),
            Statement::Assign(stmt) => self.emit_assign(stmt),
            Statement::CompoundAssign(stmt) => self.emit_compound_assign(stmt),
            Statement::Expr(stmt) => {
                self.emit_expression(&stmt.expr)?;
                Ok(())
            }
            Statement::If(stmt) => self.emit_if(stmt),
            Statement::For(stmt) => self.emit_for(stmt),
            Statement::Break(_) => self.emit_break(),
            Statement::Continue(_) => self.emit_continue(),
            Statement::Return(stmt) => self.emit_return(stmt),
            Statement::Match(stmt) => self.emit_match(stmt),
            Statement::Block(stmt) => {
                for statement in &stmt.stmts {
                    self.emit_statement(statement)?;
                    if self.terminated {
                        break;
                    }
                }
                Ok(())
            }
            Statement::Spawn(stmt) => self.emit_spawn(stmt),
        }
    }

    fn emit_return(&mut self, statement: &ReturnStmt) -> Result<(), CodegenError> {
        if self.signature.errorable {
            return self.emit_errorable_return(statement);
        }
        match (&self.signature.return_type[..], &statement.value) {
            ("void", None) => {
                self.body.push_str("  ret void\n");
            }
            (_, Some(value)) => {
                let return_type = self.signature.return_type.clone();
                let value = self.emit_expression_as(value, &return_type)?;
                let expected = self.llvm_type(&return_type)?;
                self.body
                    .push_str(&format!("  ret {expected} {}\n", value.repr));
            }
            _ => {
                return Err(CodegenError::unsupported("non-void return without a value"));
            }
        }
        self.terminated = true;
        Ok(())
    }

    fn emit_spawn(&mut self, statement: &SpawnStmt) -> Result<(), CodegenError> {
        let taskgroup = self
            .taskgroup
            .clone()
            .ok_or_else(|| CodegenError::unsupported("spawn outside taskgroup"))?;
        let Expression::Call(call) = &statement.call else {
            return Err(CodegenError::unsupported("spawn non-call"));
        };

        if let Some(callee_type) = self.expression_type_hint(&call.callee)
            && let Some(function_type) = parse_function_type(&callee_type)
        {
            return self.emit_spawn_closure_call(&taskgroup, call, &function_type);
        }

        let Expression::Ident(callee) = &call.callee else {
            return Err(CodegenError::unsupported("spawn call target"));
        };
        let signature = self
            .generator
            .info
            .functions
            .get(&callee.name)
            .ok_or_else(|| {
                CodegenError::unsupported(format!("unknown spawn function {:?}", callee.name))
            })?
            .clone();
        self.emit_spawn_function_call(&taskgroup, call, &signature)
    }

    fn emit_spawn_function_call(
        &mut self,
        taskgroup: &TaskgroupContext,
        call: &CallExpr,
        signature: &Signature,
    ) -> Result<(), CodegenError> {
        if signature.params.len() != call.args.len() {
            return Err(CodegenError::unsupported(format!(
                "spawn call arity for {:?}",
                signature.full_name
            )));
        }

        let (wrapper_name, ctx_type) = self.generator.queue_task_function_wrapper(signature)?;
        let ctx_size = self
            .generator
            .task_wrapper_context_size(&signature.params)?;
        let ctx = self.emit_alloc_bytes(&ctx_size.to_string(), true);
        for (idx, expression) in call.args.iter().enumerate() {
            let expected = signature.params[idx].clone();
            let value = self.emit_expression_as(expression, &expected)?;
            if value.type_ != expected {
                return Err(CodegenError::unsupported(format!(
                    "spawn argument {} to {:?} has type {}",
                    idx + 1,
                    signature.full_name,
                    value.type_
                )));
            }
            let field_ptr = self.temp("task.ctx.field");
            self.body.push_str(&format!(
                "  %{field_ptr} = getelementptr inbounds {ctx_type}, ptr {ctx}, i32 0, i32 {idx}\n"
            ));
            self.body.push_str(&format!(
                "  store {} {}, ptr %{field_ptr}\n",
                self.llvm_type(&expected)?,
                value.repr
            ));
        }
        self.body.push_str(&format!(
            "  call void @yar_taskgroup_spawn(ptr {}, ptr @{}, ptr {ctx})\n",
            taskgroup.handle,
            function_symbol(&wrapper_name)
        ));
        Ok(())
    }

    fn emit_spawn_closure_call(
        &mut self,
        taskgroup: &TaskgroupContext,
        call: &CallExpr,
        function_type: &FunctionType,
    ) -> Result<(), CodegenError> {
        if function_type.params.len() != call.args.len() {
            return Err(CodegenError::unsupported("spawn function value call arity"));
        }

        let callee = self.emit_expression(&call.callee)?;
        let expected_callee_type = format_function_type(
            &function_type.params,
            &function_type.return_type,
            function_type.errorable,
        );
        if callee.type_ != expected_callee_type {
            return Err(CodegenError::unsupported(format!(
                "spawn callee has type {}",
                callee.type_
            )));
        }

        let (wrapper_name, ctx_type) = self.generator.queue_task_closure_wrapper(function_type)?;
        let mut field_types = vec![expected_callee_type.clone()];
        field_types.extend(function_type.params.clone());
        let ctx_size = self.generator.task_wrapper_context_size(&field_types)?;
        let ctx = self.emit_alloc_bytes(&ctx_size.to_string(), true);
        self.emit_task_context_store(&ctx_type, &ctx, 0, &expected_callee_type, &callee)?;

        for (idx, expression) in call.args.iter().enumerate() {
            let expected = function_type.params[idx].clone();
            let value = self.emit_expression_as(expression, &expected)?;
            if value.type_ != expected {
                return Err(CodegenError::unsupported(format!(
                    "spawn function value argument {} has type {}",
                    idx + 1,
                    value.type_
                )));
            }
            self.emit_task_context_store(&ctx_type, &ctx, idx + 1, &expected, &value)?;
        }

        self.body.push_str(&format!(
            "  call void @yar_taskgroup_spawn(ptr {}, ptr @{}, ptr {ctx})\n",
            taskgroup.handle,
            function_symbol(&wrapper_name)
        ));
        Ok(())
    }

    fn emit_task_context_store(
        &mut self,
        ctx_type: &str,
        ctx: &str,
        idx: usize,
        expected: &str,
        value: &Value,
    ) -> Result<(), CodegenError> {
        let field_ptr = self.temp("task.ctx.field");
        self.body.push_str(&format!(
            "  %{field_ptr} = getelementptr inbounds {ctx_type}, ptr {ctx}, i32 0, i32 {idx}\n"
        ));
        self.body.push_str(&format!(
            "  store {} {}, ptr %{field_ptr}\n",
            self.llvm_type(expected)?,
            value.repr
        ));
        Ok(())
    }

    fn emit_taskgroup(&mut self, expression: &TaskgroupExpr) -> Result<Value, CodegenError> {
        let result_type = expression.result_type.to_string();
        let Some(element_type) = parse_slice_type(&result_type) else {
            return Err(CodegenError::unsupported("taskgroup result type"));
        };
        let elem_size = if element_type == "void" {
            0
        } else {
            self.type_size(&element_type)?
        };
        let handle = self.temp("taskgroup");
        self.body.push_str(&format!(
            "  %{handle} = call ptr @yar_taskgroup_new(i64 {elem_size})\n"
        ));

        let previous = self.taskgroup.replace(TaskgroupContext {
            handle: format!("%{handle}"),
        });
        for statement in &expression.body.stmts {
            self.emit_statement(statement)?;
            if self.terminated {
                break;
            }
        }
        self.taskgroup = previous;

        let result_slot = self.temp("taskgroup.result.slot");
        self.body
            .push_str(&format!("  %{result_slot} = alloca %yar.slice\n"));
        self.body.push_str(&format!(
            "  call void @yar_taskgroup_wait(ptr %{handle}, ptr %{result_slot})\n"
        ));
        let result = self.temp("taskgroup.result");
        self.body.push_str(&format!(
            "  %{result} = load %yar.slice, ptr %{result_slot}\n"
        ));
        Ok(Value {
            type_: result_type,
            repr: format!("%{result}"),
        })
    }

    fn emit_errorable_return(&mut self, statement: &ReturnStmt) -> Result<(), CodegenError> {
        let return_type = self.signature.return_type.clone();
        match &statement.value {
            Some(Expression::Error(error)) => {
                let result = self.emit_error_result(&return_type, &error.name)?;
                self.body.push_str(&format!(
                    "  ret {} {}\n",
                    result_type_name(&return_type),
                    result.repr
                ));
            }
            Some(value) => {
                let value = self.emit_expression_as(value, &return_type)?;
                if value.type_ == errorable_type(&return_type) {
                    self.body.push_str(&format!(
                        "  ret {} {}\n",
                        result_type_name(&return_type),
                        value.repr
                    ));
                } else {
                    if value.type_ != return_type {
                        return Err(CodegenError::unsupported(format!(
                            "errorable return value type {}",
                            value.type_
                        )));
                    }
                    let result = self.emit_success_result(&return_type, &value.repr)?;
                    self.body.push_str(&format!(
                        "  ret {} {}\n",
                        result_type_name(&return_type),
                        result.repr
                    ));
                }
            }
            None if return_type == "void" => {
                let result = self.emit_success_void_result()?;
                self.body.push_str(&format!(
                    "  ret {} {}\n",
                    result_type_name("void"),
                    result.repr
                ));
            }
            None => {
                return Err(CodegenError::unsupported(
                    "non-void errorable return without a value",
                ));
            }
        }
        self.terminated = true;
        Ok(())
    }

    fn emit_var(&mut self, statement: &VarStmt) -> Result<(), CodegenError> {
        let type_ = statement.type_ref.to_string();
        let value = match &statement.value {
            Some(value) => self.emit_expression_as(value, &type_)?,
            None => self.zero_value(&type_)?,
        };
        if value.type_ != type_ {
            return Err(CodegenError::unsupported(format!(
                "var {:?} initializer type {}",
                statement.name, value.type_
            )));
        }
        self.bind_local(&statement.name, value)
    }

    fn emit_assign(&mut self, statement: &AssignStmt) -> Result<(), CodegenError> {
        if let Expression::Index(index) = &statement.target
            && let Some(map_type) = self.expression_type_hint(&index.inner)
            && let Some((key_type, value_type)) = parse_map_type(&map_type)
        {
            let map = self.emit_expression(&index.inner)?;
            let key = self.emit_expression_as(&index.index, &key_type)?;
            let value = self.emit_expression_as(&statement.value, &value_type)?;
            self.emit_map_set(&map.repr, &key_type, &key, &value_type, &value)?;
            return Ok(());
        }

        let target = self.emit_address(&statement.target)?;
        let expected_type = target.type_.clone();
        let value = self.emit_expression_as(&statement.value, &expected_type)?;
        if value.type_ != expected_type {
            return Err(CodegenError::unsupported(format!(
                "assignment target has type {} but value has type {}",
                expected_type, value.type_
            )));
        }
        self.store_address(&target, &value)
    }

    fn emit_compound_assign(&mut self, statement: &CompoundAssignStmt) -> Result<(), CodegenError> {
        let target = self.emit_address(&statement.target)?;
        let current = self.load_address(&target)?;
        let value = self.emit_expression_as(&statement.value, &target.type_)?;
        let result = self.emit_arithmetic_value(statement.operator, current, value)?;
        self.store_address(&target, &result)
    }

    fn bind_local(&mut self, name: &str, value: Value) -> Result<(), CodegenError> {
        let llvm_type = self.llvm_type(&value.type_)?;
        let slot = self.allocate_local_slot(name, &value.type_)?;
        self.body
            .push_str(&format!("  store {llvm_type} {}, ptr {slot}\n", value.repr));
        self.locals.insert(
            name.to_string(),
            Local {
                type_: value.type_,
                ptr: slot,
            },
        );
        Ok(())
    }

    fn allocate_local_slot(&mut self, name: &str, type_: &str) -> Result<String, CodegenError> {
        if self.heap_locals.contains(name) {
            return self.emit_alloc_type(type_, false);
        }

        let llvm_type = self.llvm_type(type_)?;
        let slot = self.temp(&format!("local.{name}"));
        self.body
            .push_str(&format!("  %{slot} = alloca {llvm_type}\n"));
        Ok(format!("%{slot}"))
    }

    fn store_address(&mut self, address: &Address, value: &Value) -> Result<(), CodegenError> {
        let llvm_type = self.llvm_type(&address.type_)?;
        self.body.push_str(&format!(
            "  store {llvm_type} {}, ptr {}\n",
            value.repr, address.ptr
        ));
        Ok(())
    }

    fn emit_if(&mut self, statement: &IfStmt) -> Result<(), CodegenError> {
        let cond = self.emit_expression(&statement.cond)?;
        if cond.type_ != "bool" {
            return Err(CodegenError::unsupported("if condition type"));
        }

        let then_label = self.label("if.then");
        let after_label = self.label("if.end");
        let else_label = if statement.else_stmt.is_some() {
            self.label("if.else")
        } else {
            after_label.clone()
        };

        self.body.push_str(&format!(
            "  br i1 {}, label %{then_label}, label %{else_label}\n",
            cond.repr
        ));

        self.start_block(&then_label);
        let then_terminated = self.emit_branch_block(&statement.then_block.stmts, &after_label)?;

        let else_terminated = if let Some(else_stmt) = &statement.else_stmt {
            self.start_block(&else_label);
            self.emit_branch_statement(else_stmt, &after_label)?
        } else {
            false
        };

        if then_terminated && else_terminated {
            self.terminated = true;
            return Ok(());
        }

        self.start_block(&after_label);
        self.terminated = false;
        Ok(())
    }

    fn emit_for(&mut self, statement: &ForStmt) -> Result<(), CodegenError> {
        if let Some(init) = &statement.init {
            self.emit_statement(init)?;
            if self.terminated {
                return Ok(());
            }
        }

        let cond_label = self.label("for.cond");
        let body_label = self.label("for.body");
        let post_label = self.label("for.post");
        let end_label = self.label("for.end");
        let continue_label = if statement.post.is_some() {
            post_label.clone()
        } else {
            cond_label.clone()
        };

        self.body.push_str(&format!("  br label %{cond_label}\n"));
        self.start_block(&cond_label);
        if let Some(cond) = &statement.cond {
            let cond = self.emit_expression(cond)?;
            if cond.type_ != "bool" {
                return Err(CodegenError::unsupported("for condition type"));
            }
            self.body.push_str(&format!(
                "  br i1 {}, label %{body_label}, label %{end_label}\n",
                cond.repr
            ));
        } else {
            self.body.push_str(&format!("  br label %{body_label}\n"));
        }

        self.start_block(&body_label);
        self.loop_stack.push(LoopLabels {
            break_label: end_label.clone(),
            continue_label,
        });
        self.terminated = false;
        for stmt in &statement.body.stmts {
            self.emit_statement(stmt)?;
            if self.terminated {
                break;
            }
        }
        let body_terminated = self.terminated;
        self.loop_stack.pop();
        if !body_terminated {
            self.body.push_str(&format!("  br label %{post_label}\n"));
        }

        self.start_block(&post_label);
        self.terminated = false;
        if let Some(post) = &statement.post {
            self.emit_statement(post)?;
            if !self.terminated {
                self.body.push_str(&format!("  br label %{cond_label}\n"));
            }
        } else {
            self.body.push_str(&format!("  br label %{cond_label}\n"));
        }

        self.start_block(&end_label);
        self.terminated = false;
        Ok(())
    }

    fn emit_break(&mut self) -> Result<(), CodegenError> {
        let Some(labels) = self.loop_stack.last() else {
            return Err(CodegenError::unsupported("break outside loop"));
        };
        self.body
            .push_str(&format!("  br label %{}\n", labels.break_label));
        self.terminated = true;
        Ok(())
    }

    fn emit_continue(&mut self) -> Result<(), CodegenError> {
        let Some(labels) = self.loop_stack.last() else {
            return Err(CodegenError::unsupported("continue outside loop"));
        };
        self.body
            .push_str(&format!("  br label %{}\n", labels.continue_label));
        self.terminated = true;
        Ok(())
    }

    fn emit_match(&mut self, statement: &MatchStmt) -> Result<(), CodegenError> {
        let value = self.emit_expression(&statement.value)?;
        let enum_type = value.type_.clone();
        let enum_info = self
            .generator
            .info
            .enums
            .get(&enum_type)
            .cloned()
            .ok_or_else(|| CodegenError::unsupported(format!("match on {enum_type:?}")))?;

        let enum_llvm = self.llvm_type(&enum_type)?;
        let slot = self.temp("match.value");
        self.body
            .push_str(&format!("  %{slot} = alloca {enum_llvm}\n"));
        self.body.push_str(&format!(
            "  store {enum_llvm} {}, ptr %{slot}\n",
            value.repr
        ));
        let tag_ptr = self.temp("match.tag.ptr");
        self.body.push_str(&format!(
            "  %{tag_ptr} = getelementptr inbounds {enum_llvm}, ptr %{slot}, i32 0, i32 0\n"
        ));
        let tag = self.temp("match.tag");
        self.body
            .push_str(&format!("  %{tag} = load i32, ptr %{tag_ptr}\n"));

        let default_label = self.label("match.default");
        let end_label = self.label("match.end");
        let arm_labels = statement
            .arms
            .iter()
            .map(|_| self.label("match.case"))
            .collect::<Vec<_>>();

        self.body
            .push_str(&format!("  switch i32 %{tag}, label %{default_label} [\n"));
        for (arm, label) in statement.arms.iter().zip(&arm_labels) {
            let enum_case = enum_info
                .cases
                .iter()
                .find(|case| case.name == arm.case_name)
                .ok_or_else(|| {
                    CodegenError::unsupported(format!(
                        "unknown match case {enum_type:?}.{}",
                        arm.case_name
                    ))
                })?;
            self.body
                .push_str(&format!("    i32 {}, label %{label}\n", enum_case.tag));
        }
        self.body.push_str("  ]\n");
        self.terminated = true;

        let mut needs_end = false;
        for (arm, label) in statement.arms.iter().zip(&arm_labels) {
            self.start_block(label);
            self.terminated = false;
            let enum_case = enum_info
                .cases
                .iter()
                .find(|case| case.name == arm.case_name)
                .cloned()
                .ok_or_else(|| {
                    CodegenError::unsupported(format!(
                        "unknown match case {enum_type:?}.{}",
                        arm.case_name
                    ))
                })?;
            let previous = if !enum_case.payload_type.is_empty()
                && !arm.bind_name.is_empty()
                && !arm.bind_ignore
            {
                let payload_ptr = self.enum_payload_ptr(&slot, &enum_type)?;
                let payload = self.load_address(&Address {
                    type_: enum_case.payload_type.clone(),
                    ptr: payload_ptr,
                })?;
                let previous = self.locals.get(&arm.bind_name).cloned();
                self.bind_local(&arm.bind_name, payload)?;
                Some((arm.bind_name.clone(), previous))
            } else {
                None
            };

            for stmt in &arm.body.stmts {
                self.emit_statement(stmt)?;
                if self.terminated {
                    break;
                }
            }
            if let Some((name, previous)) = previous {
                match previous {
                    Some(local) => {
                        self.locals.insert(name, local);
                    }
                    None => {
                        self.locals.remove(&name);
                    }
                }
            }
            if !self.terminated {
                self.body.push_str(&format!("  br label %{end_label}\n"));
                needs_end = true;
            }
        }

        self.start_block(&default_label);
        self.terminated = false;
        if let Some(else_body) = &statement.else_body {
            for stmt in &else_body.stmts {
                self.emit_statement(stmt)?;
                if self.terminated {
                    break;
                }
            }
            if !self.terminated {
                self.body.push_str(&format!("  br label %{end_label}\n"));
                needs_end = true;
            }
        } else {
            self.body.push_str("  unreachable\n");
            self.terminated = true;
        }

        if needs_end {
            self.start_block(&end_label);
            self.terminated = false;
        }
        Ok(())
    }

    fn emit_branch_statement(
        &mut self,
        statement: &Statement,
        after_label: &str,
    ) -> Result<bool, CodegenError> {
        let previous = self.terminated;
        self.terminated = false;
        self.emit_statement(statement)?;
        let branch_terminated = self.terminated;
        if !branch_terminated {
            self.body.push_str(&format!("  br label %{after_label}\n"));
        }
        self.terminated = previous;
        Ok(branch_terminated)
    }

    fn emit_branch_block(
        &mut self,
        statements: &[Statement],
        after_label: &str,
    ) -> Result<bool, CodegenError> {
        let previous = self.terminated;
        self.terminated = false;
        for statement in statements {
            self.emit_statement(statement)?;
            if self.terminated {
                break;
            }
        }
        let branch_terminated = self.terminated;
        if !branch_terminated {
            self.body.push_str(&format!("  br label %{after_label}\n"));
        }
        self.terminated = previous;
        Ok(branch_terminated)
    }

    fn emit_fallthrough_return(&mut self) -> Result<(), CodegenError> {
        match self.signature.return_type.as_str() {
            "void" => self.body.push_str("  ret void\n"),
            other => {
                return Err(CodegenError::unsupported(format!(
                    "fallthrough return for {other}"
                )));
            }
        }
        self.terminated = true;
        Ok(())
    }

    fn emit_expression(&mut self, expression: &Expression) -> Result<Value, CodegenError> {
        match expression {
            Expression::Ident(expr) => {
                let local = self.locals.get(&expr.name).cloned().ok_or_else(|| {
                    CodegenError::unsupported(format!("unknown local {:?}", expr.name))
                })?;
                self.load_local(&expr.name, &local)
            }
            Expression::Int(expr) => Ok(Value {
                type_: "i32".to_string(),
                repr: expr.value.to_string(),
            }),
            Expression::Char(expr) => Ok(Value {
                type_: "i32".to_string(),
                repr: u32::from(expr.value).to_string(),
            }),
            Expression::Bool(expr) => Ok(Value {
                type_: "bool".to_string(),
                repr: if expr.value { "true" } else { "false" }.to_string(),
            }),
            Expression::Nil(_) => Ok(Value {
                type_: "nil".to_string(),
                repr: "null".to_string(),
            }),
            Expression::String(expr) => self.emit_string_literal(&expr.value),
            Expression::Error(expr) => {
                let code = self.generator.error_code(&expr.name)?;
                Ok(Value {
                    type_: "error".to_string(),
                    repr: code.to_string(),
                })
            }
            Expression::Group(expr) => self.emit_expression(&expr.inner),
            Expression::Unary(expr) => self.emit_unary(expr),
            Expression::Binary(expr) => self.emit_binary(expr),
            Expression::Selector(expr) => self.emit_selector(expr),
            Expression::Index(expr) => self.emit_index(expr),
            Expression::Slice(expr) => self.emit_slice(expr),
            Expression::StructLiteral(expr) => self.emit_struct_literal(expr),
            Expression::ArrayLiteral(expr) => self.emit_array_literal(expr),
            Expression::SliceLiteral(expr) => self.emit_slice_literal(expr),
            Expression::MapLiteral(expr) => self.emit_map_literal(expr),
            Expression::FunctionLiteral(expr) => self.emit_function_literal(expr),
            Expression::Taskgroup(expr) => self.emit_taskgroup(expr),
            Expression::Call(expr) => self.emit_call(expr),
            Expression::Propagate(expr) => self.emit_propagate(expr),
            Expression::Handle(expr) => self.emit_handle(expr),
            other => Err(CodegenError::unsupported(expression_name(other))),
        }
    }

    fn load_local(&mut self, name: &str, local: &Local) -> Result<Value, CodegenError> {
        let llvm_type = self.llvm_type(&local.type_)?;
        let result = self.temp(&format!("load.{name}"));
        self.body.push_str(&format!(
            "  %{result} = load {llvm_type}, ptr {}\n",
            local.ptr
        ));
        Ok(Value {
            type_: local.type_.clone(),
            repr: format!("%{result}"),
        })
    }

    fn load_address(&mut self, address: &Address) -> Result<Value, CodegenError> {
        let llvm_type = self.llvm_type(&address.type_)?;
        let result = self.temp("load");
        self.body.push_str(&format!(
            "  %{result} = load {llvm_type}, ptr {}\n",
            address.ptr
        ));
        Ok(Value {
            type_: address.type_.clone(),
            repr: format!("%{result}"),
        })
    }

    fn emit_pointer_check(&mut self, pointer: &str) {
        self.body
            .push_str(&format!("  call void @yar_pointer_check(ptr {pointer})\n"));
    }

    fn emit_address(&mut self, expression: &Expression) -> Result<Address, CodegenError> {
        match expression {
            Expression::Ident(expr) => {
                let local = self.locals.get(&expr.name).cloned().ok_or_else(|| {
                    CodegenError::unsupported(format!("unknown local {:?}", expr.name))
                })?;
                Ok(Address {
                    type_: local.type_,
                    ptr: local.ptr,
                })
            }
            Expression::Group(expr) => self.emit_address(&expr.inner),
            Expression::Unary(expr) if expr.operator == Kind::Star => {
                let value = self.emit_expression(&expr.inner)?;
                let Some(element_type) = parse_pointer_type(&value.type_) else {
                    return Err(CodegenError::unsupported("dereference address operand"));
                };
                self.emit_pointer_check(&value.repr);
                Ok(Address {
                    type_: element_type,
                    ptr: value.repr,
                })
            }
            Expression::Selector(expr) => {
                let mut base = self.emit_aggregate_address(&expr.inner)?;
                if let Some(element_type) = parse_pointer_type(&base.type_) {
                    let pointer = self.load_address(&base)?;
                    self.emit_pointer_check(&pointer.repr);
                    base = Address {
                        type_: element_type,
                        ptr: pointer.repr,
                    };
                }
                let (idx, field_type) = self.generator.struct_field(&base.type_, &expr.name)?;
                let field_ptr = self.temp("field.ptr");
                self.body.push_str(&format!(
                    "  %{field_ptr} = getelementptr inbounds {}, ptr {}, i32 0, i32 {idx}\n",
                    self.llvm_type(&base.type_)?,
                    base.ptr
                ));
                Ok(Address {
                    type_: field_type,
                    ptr: format!("%{field_ptr}"),
                })
            }
            Expression::Index(expr) => {
                let base = self.emit_aggregate_address(&expr.inner)?;
                let index = self.emit_expression(&expr.index)?;
                if let Some((len, element_type)) = parse_array_type(&base.type_) {
                    let len =
                        i64::try_from(len).map_err(|_| CodegenError::unsupported("array size"))?;
                    let index64 = self.int_to_i64(&index)?;
                    self.body.push_str(&format!(
                        "  call void @yar_array_index_check(i64 {index64}, i64 {len})\n"
                    ));
                    let element_ptr = self.temp("elem.ptr");
                    self.body.push_str(&format!(
                        "  %{element_ptr} = getelementptr inbounds {}, ptr {}, i32 0, i64 {index64}\n",
                        self.llvm_type(&base.type_)?,
                        base.ptr
                    ));
                    return Ok(Address {
                        type_: element_type,
                        ptr: format!("%{element_ptr}"),
                    });
                }
                let Some(element_type) = parse_slice_type(&base.type_) else {
                    return Err(CodegenError::unsupported("non-aggregate address index"));
                };
                let slice = self.load_address(&base)?;
                let data = self.extract_slice_field(&slice.repr, 0, "slice.data")?;
                let len = self.extract_slice_field(&slice.repr, 1, "slice.len")?;
                let index64 = self.int_to_i64(&index)?;
                let len64 = self.int_to_i64(&len)?;
                self.body.push_str(&format!(
                    "  call void @yar_slice_index_check(i64 {index64}, i64 {len64})\n"
                ));
                let element_ptr = self.temp("elem.ptr");
                self.body.push_str(&format!(
                    "  %{element_ptr} = getelementptr {}, ptr {}, i64 {index64}\n",
                    self.llvm_type(&element_type)?,
                    data.repr
                ));
                Ok(Address {
                    type_: element_type,
                    ptr: format!("%{element_ptr}"),
                })
            }
            other => Err(CodegenError::unsupported(format!(
                "address of {}",
                expression_name(other)
            ))),
        }
    }

    fn emit_aggregate_address(&mut self, expression: &Expression) -> Result<Address, CodegenError> {
        if let Ok(address) = self.emit_address(expression) {
            return Ok(address);
        }

        let value = self.emit_expression(expression)?;
        let llvm_type = self.llvm_type(&value.type_)?;
        let slot = self.temp("agg.slot");
        self.body
            .push_str(&format!("  %{slot} = alloca {llvm_type}\n"));
        self.body.push_str(&format!(
            "  store {llvm_type} {}, ptr %{slot}\n",
            value.repr
        ));
        Ok(Address {
            type_: value.type_,
            ptr: format!("%{slot}"),
        })
    }

    fn emit_address_of_expr(
        &mut self,
        expression: &Expression,
    ) -> Result<(String, String), CodegenError> {
        if let Ok(address) = self.emit_address(expression) {
            return Ok((address.ptr, address.type_));
        }

        let value = self.emit_expression(expression)?;
        let ptr = self.emit_alloc_type(&value.type_, false)?;
        self.body.push_str(&format!(
            "  store {} {}, ptr {ptr}\n",
            self.llvm_type(&value.type_)?,
            value.repr
        ));
        Ok((ptr, value.type_))
    }

    fn emit_string_literal(&mut self, value: &str) -> Result<Value, CodegenError> {
        let global = self.generator.string_name(value);
        let first = self.temp("str.ptr");
        self.body.push_str(&format!(
            "  %{first} = insertvalue %yar.str undef, ptr @{global}, 0\n"
        ));
        let second = self.temp("str.len");
        self.body.push_str(&format!(
            "  %{second} = insertvalue %yar.str %{first}, i64 {}, 1\n",
            value.len()
        ));
        Ok(Value {
            type_: "str".to_string(),
            repr: format!("%{second}"),
        })
    }

    fn emit_unary(&mut self, expression: &UnaryExpr) -> Result<Value, CodegenError> {
        match expression.operator {
            Kind::Amp => {
                let (ptr, type_) = self.emit_address_of_expr(&expression.inner)?;
                Ok(Value {
                    type_: format!("*{type_}"),
                    repr: ptr,
                })
            }
            Kind::Star => {
                let value = self.emit_expression(&expression.inner)?;
                let Some(element_type) = parse_pointer_type(&value.type_) else {
                    return Err(CodegenError::unsupported("dereference operand type"));
                };
                self.emit_pointer_check(&value.repr);
                let result = self.temp("deref");
                self.body.push_str(&format!(
                    "  %{result} = load {}, ptr {}\n",
                    self.llvm_type(&element_type)?,
                    value.repr
                ));
                Ok(Value {
                    type_: element_type,
                    repr: format!("%{result}"),
                })
            }
            Kind::Bang => {
                let value = self.emit_expression(&expression.inner)?;
                if value.type_ != "bool" {
                    return Err(CodegenError::unsupported("unary ! operand type"));
                }
                let result = self.temp("not");
                self.body
                    .push_str(&format!("  %{result} = xor i1 {}, true\n", value.repr));
                Ok(Value {
                    type_: "bool".to_string(),
                    repr: format!("%{result}"),
                })
            }
            Kind::Minus => {
                let value = self.emit_expression(&expression.inner)?;
                if !matches!(value.type_.as_str(), "i32" | "i64") {
                    return Err(CodegenError::unsupported("unary - operand type"));
                }
                let result = self.temp("neg");
                self.body.push_str(&format!(
                    "  %{result} = sub {} 0, {}\n",
                    self.llvm_type(&value.type_)?,
                    value.repr
                ));
                Ok(Value {
                    type_: value.type_,
                    repr: format!("%{result}"),
                })
            }
            _ => Err(CodegenError::unsupported("unary operator")),
        }
    }

    fn emit_struct_literal(&mut self, literal: &StructLiteralExpr) -> Result<Value, CodegenError> {
        let type_ = literal.type_ref.to_string();
        if let Some((enum_type, enum_case)) = self.generator.enum_case_type(&type_)? {
            let payload = self.emit_struct_value(&type_, &literal.fields)?;
            return self.emit_enum_value(&enum_type, &enum_case, Some(&payload));
        }
        self.emit_struct_value(&type_, &literal.fields)
    }

    fn emit_struct_value(
        &mut self,
        type_: &str,
        fields: &[StructLiteralField],
    ) -> Result<Value, CodegenError> {
        let aggregate_type = self.llvm_type(type_)?;
        let mut value = "zeroinitializer".to_string();
        for field in fields {
            let (idx, field_type) = self.generator.struct_field(type_, &field.name)?;
            let field_value = self.emit_expression_as(&field.value, &field_type)?;
            if field_value.type_ != field_type {
                return Err(CodegenError::unsupported(format!(
                    "struct field {:?} has type {}",
                    field.name, field_value.type_
                )));
            }
            let next = self.temp("struct");
            self.body.push_str(&format!(
                "  %{next} = insertvalue {aggregate_type} {value}, {} {}, {idx}\n",
                self.llvm_type(&field_type)?,
                field_value.repr
            ));
            value = format!("%{next}");
        }
        Ok(Value {
            type_: type_.to_string(),
            repr: value,
        })
    }

    fn emit_struct_field_value(
        &mut self,
        type_: &str,
        field_value: &Value,
        field_index: usize,
    ) -> Result<Value, CodegenError> {
        let aggregate_type = self.llvm_type(type_)?;
        let result = self.temp("struct");
        self.body.push_str(&format!(
            "  %{result} = insertvalue {aggregate_type} zeroinitializer, {} {}, {field_index}\n",
            self.llvm_type(&field_value.type_)?,
            field_value.repr
        ));
        Ok(Value {
            type_: type_.to_string(),
            repr: format!("%{result}"),
        })
    }

    fn emit_enum_value(
        &mut self,
        enum_type: &str,
        enum_case: &EnumCaseInfo,
        payload: Option<&Value>,
    ) -> Result<Value, CodegenError> {
        let enum_llvm = self.llvm_type(enum_type)?;
        let slot = self.temp("enum.value");
        self.body
            .push_str(&format!("  %{slot} = alloca {enum_llvm}\n"));
        self.body.push_str(&format!(
            "  store {enum_llvm} zeroinitializer, ptr %{slot}\n"
        ));
        let tag_ptr = self.temp("enum.tag.ptr");
        self.body.push_str(&format!(
            "  %{tag_ptr} = getelementptr inbounds {enum_llvm}, ptr %{slot}, i32 0, i32 0\n"
        ));
        self.body
            .push_str(&format!("  store i32 {}, ptr %{tag_ptr}\n", enum_case.tag));

        if let Some(payload) = payload {
            if payload.type_ != enum_case.payload_type {
                return Err(CodegenError::unsupported(format!(
                    "enum payload has type {}",
                    payload.type_
                )));
            }
            let payload_ptr = self.enum_payload_ptr(&slot, enum_type)?;
            self.body.push_str(&format!(
                "  store {} {}, ptr {payload_ptr}\n",
                self.llvm_type(&payload.type_)?,
                payload.repr
            ));
        } else if !enum_case.payload_type.is_empty() {
            return Err(CodegenError::unsupported(format!(
                "missing enum payload {enum_type}.{}",
                enum_case.name
            )));
        }

        let result = self.temp("enum.load");
        self.body
            .push_str(&format!("  %{result} = load {enum_llvm}, ptr %{slot}\n"));
        Ok(Value {
            type_: enum_type.to_string(),
            repr: format!("%{result}"),
        })
    }

    fn emit_enum_tag_value(&mut self, enum_type: &str, tag: &str) -> Result<Value, CodegenError> {
        let enum_llvm = self.llvm_type(enum_type)?;
        let result = self.temp("enum.tag");
        self.body.push_str(&format!(
            "  %{result} = insertvalue {enum_llvm} zeroinitializer, i32 {tag}, 0\n"
        ));
        Ok(Value {
            type_: enum_type.to_string(),
            repr: format!("%{result}"),
        })
    }

    fn emit_array_literal(&mut self, literal: &ArrayLiteralExpr) -> Result<Value, CodegenError> {
        let type_ = literal.type_ref.to_string();
        let aggregate_type = self.llvm_type(&type_)?;
        let Some((_, element_type)) = parse_array_type(&type_) else {
            return Err(CodegenError::unsupported("array literal type"));
        };
        let mut value = "zeroinitializer".to_string();
        for (idx, element) in literal.elements.iter().enumerate() {
            let element_value = self.emit_expression_as(element, &element_type)?;
            if element_value.type_ != element_type {
                return Err(CodegenError::unsupported(format!(
                    "array element has type {}",
                    element_value.type_
                )));
            }
            let next = self.temp("array");
            self.body.push_str(&format!(
                "  %{next} = insertvalue {aggregate_type} {value}, {} {}, {idx}\n",
                self.llvm_type(&element_type)?,
                element_value.repr
            ));
            value = format!("%{next}");
        }
        Ok(Value { type_, repr: value })
    }

    fn emit_slice_literal(&mut self, literal: &SliceLiteralExpr) -> Result<Value, CodegenError> {
        let type_ = literal.type_ref.to_string();
        let Some(element_type) = parse_slice_type(&type_) else {
            return Err(CodegenError::unsupported("slice literal type"));
        };
        if literal.elements.is_empty() {
            return Ok(Value {
                type_,
                repr: "zeroinitializer".to_string(),
            });
        }

        let len = literal.elements.len();
        let alloc_size = self.emit_array_alloc_size(&element_type, len)?;
        let data = self.emit_alloc_bytes(&alloc_size, false);
        for (idx, element) in literal.elements.iter().enumerate() {
            let value = self.emit_expression_as(element, &element_type)?;
            if value.type_ != element_type {
                return Err(CodegenError::unsupported(format!(
                    "slice element has type {}",
                    value.type_
                )));
            }
            let ptr = self.temp("slice.elem.ptr");
            self.body.push_str(&format!(
                "  %{ptr} = getelementptr {}, ptr {data}, i64 {idx}\n",
                self.llvm_type(&element_type)?
            ));
            self.body.push_str(&format!(
                "  store {} {}, ptr %{ptr}\n",
                self.llvm_type(&element_type)?,
                value.repr
            ));
        }

        Ok(Value {
            type_,
            repr: self.emit_slice_value(&data, &len.to_string(), &len.to_string()),
        })
    }

    fn emit_map_literal(&mut self, literal: &MapLiteralExpr) -> Result<Value, CodegenError> {
        let type_ = literal.type_ref.to_string();
        let Some((key_type, value_type)) = parse_map_type(&type_) else {
            return Err(CodegenError::unsupported("map literal type"));
        };

        let map = self.temp("map.new");
        let key_kind = map_key_kind(&key_type)?;
        let key_size = self.type_size(&key_type)?;
        let value_size = self.type_size(&value_type)?;
        self.body.push_str(&format!(
            "  %{map} = call ptr @yar_map_new(i32 {key_kind}, i32 {key_size}, i32 {value_size})\n"
        ));

        for pair in &literal.pairs {
            let key = self.emit_expression_as(&pair.key, &key_type)?;
            let value = self.emit_expression_as(&pair.value, &value_type)?;
            self.emit_map_set(&format!("%{map}"), &key_type, &key, &value_type, &value)?;
        }

        Ok(Value {
            type_,
            repr: format!("%{map}"),
        })
    }

    fn emit_function_literal(
        &mut self,
        literal: &FunctionLiteralExpr,
    ) -> Result<Value, CodegenError> {
        let key = function_literal_key(literal);
        let name = self
            .generator
            .literal_names
            .get(&key)
            .cloned()
            .ok_or_else(|| CodegenError::unsupported("missing function literal name"))?;
        let info = self
            .generator
            .info
            .function_literals
            .get(&key)
            .ok_or_else(|| CodegenError::unsupported("missing function literal info"))?
            .clone();

        let env = if info.captures.is_empty() {
            "null".to_string()
        } else {
            let size = self.closure_env_size(&info.captures)?;
            let env = self.emit_alloc_bytes(&size.to_string(), false);
            let env_type = self.closure_env_type_literal(&info.captures)?;
            for (idx, capture) in info.captures.iter().enumerate() {
                let local = self.locals.get(&capture.name).cloned().ok_or_else(|| {
                    CodegenError::unsupported(format!("missing captured local {:?}", capture.name))
                })?;
                let value = self.load_local(&capture.name, &local)?;
                let field = self.temp("closure.env.ptr");
                self.body.push_str(&format!(
                    "  %{field} = getelementptr inbounds {env_type}, ptr {env}, i32 0, i32 {idx}\n"
                ));
                self.body.push_str(&format!(
                    "  store {} {}, ptr %{field}\n",
                    self.llvm_type(&capture.type_)?,
                    value.repr
                ));
            }
            env
        };

        Ok(Value {
            type_: format_function_type(
                &info.signature.params,
                &info.signature.return_type,
                info.signature.errorable,
            ),
            repr: self.emit_closure_value(&format!("@{}", function_symbol(&name)), &env),
        })
    }

    fn emit_binary(&mut self, expression: &BinaryExpr) -> Result<Value, CodegenError> {
        if matches!(expression.operator, Kind::AmpAmp | Kind::PipePipe) {
            return self.emit_logical(expression);
        }

        let left_start = self.body.len();
        let mut left = self.emit_expression(&expression.left)?;
        let right_start = self.body.len();
        let mut right = if is_untyped_integer_expression(&expression.right)
            && matches!(left.type_.as_str(), "i32" | "i64")
        {
            self.emit_expression_as(&expression.right, &left.type_)?
        } else {
            self.emit_expression(&expression.right)?
        };

        if left.type_ != right.type_ {
            if right.type_ == "i64" && is_untyped_integer_expression(&expression.left) {
                // Retype the already-emitted pure left operand without moving the
                // right operand ahead of it in runtime evaluation order.
                let right_body = self.body.split_off(right_start);
                self.body.truncate(left_start);
                left = self.emit_expression_as(&expression.left, "i64")?;
                self.body.push_str(&right_body);
            } else if matches!(expression.operator, Kind::EqualEqual | Kind::BangEqual) {
                if left.type_ == "nil" && parse_pointer_type(&right.type_).is_some() {
                    left.type_ = right.type_.clone();
                } else if right.type_ == "nil" && parse_pointer_type(&left.type_).is_some() {
                    right.type_ = left.type_.clone();
                }
            }
        }

        if left.type_ != right.type_ {
            return Err(CodegenError::unsupported("mixed-type binary expression"));
        }

        if matches!(
            expression.operator,
            Kind::EqualEqual
                | Kind::BangEqual
                | Kind::Less
                | Kind::LessEqual
                | Kind::Greater
                | Kind::GreaterEqual
        ) {
            return self.emit_comparison(expression.operator, left, right);
        }

        self.emit_arithmetic_value(expression.operator, left, right)
    }

    fn emit_arithmetic_value(
        &mut self,
        operator: Kind,
        left: Value,
        right: Value,
    ) -> Result<Value, CodegenError> {
        if left.type_ != right.type_ {
            return Err(CodegenError::unsupported(
                "mixed-type arithmetic expression",
            ));
        }
        if !matches!(left.type_.as_str(), "i32" | "i64") {
            if operator == Kind::Plus && left.type_ == "str" {
                return self.emit_string_concat(left, right);
            }
            return Err(CodegenError::unsupported(
                "non-integer arithmetic expression",
            ));
        }
        if matches!(operator, Kind::Slash | Kind::Percent) {
            self.emit_divrem_check(&left, &right)?;
        }
        let op = arithmetic_op(operator)?;
        let result = self.temp("bin");
        self.body.push_str(&format!(
            "  %{result} = {op} {} {}, {}\n",
            self.llvm_type(&left.type_)?,
            left.repr,
            right.repr
        ));
        Ok(Value {
            type_: left.type_,
            repr: format!("%{result}"),
        })
    }

    fn emit_divrem_check(&mut self, left: &Value, right: &Value) -> Result<(), CodegenError> {
        let helper = match left.type_.as_str() {
            "i32" => "yar_i32_divrem_check",
            "i64" => "yar_i64_divrem_check",
            _ => return Err(CodegenError::unsupported("division operand type")),
        };
        let llvm_type = self.llvm_type(&left.type_)?;
        self.body.push_str(&format!(
            "  call void @{helper}({llvm_type} {}, {llvm_type} {})\n",
            left.repr, right.repr
        ));
        Ok(())
    }

    fn emit_string_concat(&mut self, left: Value, right: Value) -> Result<Value, CodegenError> {
        if right.type_ != "str" {
            return Err(CodegenError::unsupported("string concat rhs type"));
        }
        let left_ptr = self.temp("str.left.ptr");
        let left_len = self.temp("str.left.len");
        let right_ptr = self.temp("str.right.ptr");
        let right_len = self.temp("str.right.len");
        let result = self.temp("str.concat");
        self.body.push_str(&format!(
            "  %{left_ptr} = extractvalue %yar.str {}, 0\n",
            left.repr
        ));
        self.body.push_str(&format!(
            "  %{left_len} = extractvalue %yar.str {}, 1\n",
            left.repr
        ));
        self.body.push_str(&format!(
            "  %{right_ptr} = extractvalue %yar.str {}, 0\n",
            right.repr
        ));
        self.body.push_str(&format!(
            "  %{right_len} = extractvalue %yar.str {}, 1\n",
            right.repr
        ));
        let result_slot = self.temp("str.concat.slot");
        self.body
            .push_str(&format!("  %{result_slot} = alloca %yar.str\n"));
        self.body.push_str(&format!(
            "  call void @yar_str_concat(ptr %{left_ptr}, i64 %{left_len}, ptr %{right_ptr}, i64 %{right_len}, ptr %{result_slot})\n"
        ));
        self.body.push_str(&format!(
            "  %{result} = load %yar.str, ptr %{result_slot}\n"
        ));
        Ok(Value {
            type_: "str".to_string(),
            repr: format!("%{result}"),
        })
    }

    fn emit_comparison(
        &mut self,
        operator: Kind,
        left: Value,
        right: Value,
    ) -> Result<Value, CodegenError> {
        if left.type_ == "str" {
            return self.emit_string_comparison(operator, left, right);
        }
        if parse_pointer_type(&left.type_).is_some() || parse_chan_type(&left.type_).is_some() {
            let predicate = match operator {
                Kind::EqualEqual => "eq",
                Kind::BangEqual => "ne",
                _ => return Err(CodegenError::unsupported("pointer comparison operator")),
            };
            let result = self.temp("ptr.cmp");
            self.body.push_str(&format!(
                "  %{result} = icmp {predicate} ptr {}, {}\n",
                left.repr, right.repr
            ));
            return Ok(Value {
                type_: "bool".to_string(),
                repr: format!("%{result}"),
            });
        }
        let predicate = match (operator, left.type_.as_str()) {
            (Kind::EqualEqual, "i32" | "i64" | "bool" | "error") => "eq",
            (Kind::BangEqual, "i32" | "i64" | "bool" | "error") => "ne",
            (Kind::Less, "i32" | "i64") => "slt",
            (Kind::LessEqual, "i32" | "i64") => "sle",
            (Kind::Greater, "i32" | "i64") => "sgt",
            (Kind::GreaterEqual, "i32" | "i64") => "sge",
            _ => return Err(CodegenError::unsupported("comparison operands")),
        };
        let result = self.temp("cmp");
        self.body.push_str(&format!(
            "  %{result} = icmp {predicate} {} {}, {}\n",
            self.llvm_type(&left.type_)?,
            left.repr,
            right.repr
        ));
        Ok(Value {
            type_: "bool".to_string(),
            repr: format!("%{result}"),
        })
    }

    fn emit_string_comparison(
        &mut self,
        operator: Kind,
        left: Value,
        right: Value,
    ) -> Result<Value, CodegenError> {
        if right.type_ != "str" {
            return Err(CodegenError::unsupported("string comparison rhs type"));
        }
        let predicate = match operator {
            Kind::EqualEqual => "ne",
            Kind::BangEqual => "eq",
            _ => return Err(CodegenError::unsupported("string comparison operator")),
        };
        let left_ptr = self.temp("str.eq.left.ptr");
        let left_len = self.temp("str.eq.left.len");
        let right_ptr = self.temp("str.eq.right.ptr");
        let right_len = self.temp("str.eq.right.len");
        let raw = self.temp("str.eq.raw");
        let result = self.temp("str.eq");
        self.body.push_str(&format!(
            "  %{left_ptr} = extractvalue %yar.str {}, 0\n",
            left.repr
        ));
        self.body.push_str(&format!(
            "  %{left_len} = extractvalue %yar.str {}, 1\n",
            left.repr
        ));
        self.body.push_str(&format!(
            "  %{right_ptr} = extractvalue %yar.str {}, 0\n",
            right.repr
        ));
        self.body.push_str(&format!(
            "  %{right_len} = extractvalue %yar.str {}, 1\n",
            right.repr
        ));
        self.body.push_str(&format!(
            "  %{raw} = call i32 @yar_str_equal(ptr %{left_ptr}, i64 %{left_len}, ptr %{right_ptr}, i64 %{right_len})\n"
        ));
        self.body
            .push_str(&format!("  %{result} = icmp {predicate} i32 %{raw}, 0\n"));
        Ok(Value {
            type_: "bool".to_string(),
            repr: format!("%{result}"),
        })
    }

    fn emit_selector(&mut self, expression: &SelectorExpr) -> Result<Value, CodegenError> {
        if let Some(value) = self.emit_enum_case_selector(expression)? {
            return Ok(value);
        }
        let address = self.emit_address(&Expression::Selector(Box::new(expression.clone())))?;
        self.load_address(&address)
    }

    fn emit_enum_case_selector(
        &mut self,
        expression: &SelectorExpr,
    ) -> Result<Option<Value>, CodegenError> {
        let Expression::Ident(enum_ident) = &expression.inner else {
            return Ok(None);
        };
        if !self.generator.info.enums.contains_key(&enum_ident.name) {
            return Ok(None);
        }
        let enum_case = self
            .generator
            .enum_case(&enum_ident.name, &expression.name)?;
        if !enum_case.payload_type.is_empty() {
            return Err(CodegenError::unsupported(format!(
                "enum payload case {}.{} requires a payload",
                enum_ident.name, expression.name
            )));
        }
        self.emit_enum_value(&enum_ident.name, &enum_case, None)
            .map(Some)
    }

    fn emit_index(&mut self, expression: &IndexExpr) -> Result<Value, CodegenError> {
        if let Some(map_type) = self.expression_type_hint(&expression.inner)
            && let Some((key_type, value_type)) = parse_map_type(&map_type)
        {
            return self.emit_map_lookup(expression, &key_type, &value_type);
        }

        if let Ok(address) = self.emit_address(&Expression::Index(Box::new(expression.clone()))) {
            return self.load_address(&address);
        }

        let inner = self.emit_expression(&expression.inner)?;
        if inner.type_ != "str" {
            return Err(CodegenError::unsupported(format!(
                "index for {}",
                inner.type_
            )));
        }
        let index = self.emit_expression(&expression.index)?;
        let index64 = self.int_to_i64(&index)?;

        let str_ptr = self.temp("str.ptr");
        let str_len = self.temp("str.len");
        self.body.push_str(&format!(
            "  %{str_ptr} = extractvalue %yar.str {}, 0\n",
            inner.repr
        ));
        self.body.push_str(&format!(
            "  %{str_len} = extractvalue %yar.str {}, 1\n",
            inner.repr
        ));
        self.body.push_str(&format!(
            "  call void @yar_str_index_check(i64 {index64}, i64 %{str_len})\n"
        ));

        let byte_ptr = self.temp("str.byte.ptr");
        self.body.push_str(&format!(
            "  %{byte_ptr} = getelementptr i8, ptr %{str_ptr}, i64 {index64}\n"
        ));
        let byte = self.temp("str.byte");
        self.body
            .push_str(&format!("  %{byte} = load i8, ptr %{byte_ptr}\n"));
        let result = self.temp("str.byte.i32");
        self.body
            .push_str(&format!("  %{result} = zext i8 %{byte} to i32\n"));
        Ok(Value {
            type_: "i32".to_string(),
            repr: format!("%{result}"),
        })
    }

    fn emit_slice(&mut self, expression: &SliceExpr) -> Result<Value, CodegenError> {
        let inner = self.emit_expression(&expression.inner)?;
        if let Some(element_type) = parse_slice_type(&inner.type_) {
            return self.emit_slice_slice(expression, inner, &element_type);
        }
        if inner.type_ == "str" {
            return self.emit_string_slice(expression, inner);
        }

        Err(CodegenError::unsupported("non-sliceable slice"))
    }

    fn emit_string_slice(
        &mut self,
        expression: &SliceExpr,
        inner: Value,
    ) -> Result<Value, CodegenError> {
        let str_ptr = self.temp("str.ptr");
        let str_len = self.temp("str.len");
        self.body.push_str(&format!(
            "  %{str_ptr} = extractvalue %yar.str {}, 0\n",
            inner.repr
        ));
        self.body.push_str(&format!(
            "  %{str_len} = extractvalue %yar.str {}, 1\n",
            inner.repr
        ));

        let start = match &expression.start {
            Some(start) => {
                let value = self.emit_expression(start)?;
                self.int_to_i64(&value)?
            }
            None => "0".to_string(),
        };
        let end = match &expression.end {
            Some(end) => {
                let value = self.emit_expression(end)?;
                self.int_to_i64(&value)?
            }
            None => format!("%{str_len}"),
        };
        self.body.push_str(&format!(
            "  call void @yar_slice_range_check(i64 {start}, i64 {end}, i64 %{str_len})\n"
        ));

        let new_ptr = self.temp("str.slice.ptr");
        self.body.push_str(&format!(
            "  %{new_ptr} = getelementptr i8, ptr %{str_ptr}, i64 {start}\n"
        ));
        let new_len = self.temp("str.slice.len");
        self.body
            .push_str(&format!("  %{new_len} = sub i64 {end}, {start}\n"));
        let first = self.temp("str.slice");
        self.body.push_str(&format!(
            "  %{first} = insertvalue %yar.str undef, ptr %{new_ptr}, 0\n"
        ));
        let second = self.temp("str.slice");
        self.body.push_str(&format!(
            "  %{second} = insertvalue %yar.str %{first}, i64 %{new_len}, 1\n"
        ));
        Ok(Value {
            type_: "str".to_string(),
            repr: format!("%{second}"),
        })
    }

    fn emit_slice_slice(
        &mut self,
        expression: &SliceExpr,
        inner: Value,
        element_type: &str,
    ) -> Result<Value, CodegenError> {
        let data = self.extract_slice_field(&inner.repr, 0, "slice.data")?;
        let len = self.extract_slice_field(&inner.repr, 1, "slice.len")?;
        let cap = self.extract_slice_field(&inner.repr, 2, "slice.cap")?;
        let len64 = self.int_to_i64(&len)?;
        let cap64 = self.int_to_i64(&cap)?;
        let start = match &expression.start {
            Some(start) => {
                let value = self.emit_expression(start)?;
                self.int_to_i64(&value)?
            }
            None => "0".to_string(),
        };
        let end = match &expression.end {
            Some(end) => {
                let value = self.emit_expression(end)?;
                self.int_to_i64(&value)?
            }
            None => len64.clone(),
        };
        self.body.push_str(&format!(
            "  call void @yar_slice_range_check(i64 {start}, i64 {end}, i64 {len64})\n"
        ));
        let new_ptr = self.temp("slice.ptr");
        self.body.push_str(&format!(
            "  %{new_ptr} = getelementptr {}, ptr {}, i64 {start}\n",
            self.llvm_type(element_type)?,
            data.repr
        ));
        let new_len64 = self.temp("slice.len64");
        self.body
            .push_str(&format!("  %{new_len64} = sub i64 {end}, {start}\n"));
        let new_len = self.temp("slice.len");
        self.body
            .push_str(&format!("  %{new_len} = trunc i64 %{new_len64} to i32\n"));
        let new_cap64 = self.temp("slice.cap64");
        self.body
            .push_str(&format!("  %{new_cap64} = sub i64 {cap64}, {start}\n"));
        let new_cap = self.temp("slice.cap");
        self.body
            .push_str(&format!("  %{new_cap} = trunc i64 %{new_cap64} to i32\n"));

        Ok(Value {
            type_: inner.type_,
            repr: self.emit_slice_value(
                &format!("%{new_ptr}"),
                &format!("%{new_len}"),
                &format!("%{new_cap}"),
            ),
        })
    }

    fn emit_logical(&mut self, expression: &BinaryExpr) -> Result<Value, CodegenError> {
        let left = self.emit_expression(&expression.left)?;
        if left.type_ != "bool" {
            return Err(CodegenError::unsupported("logical lhs type"));
        }

        let slot = self.temp("logic.slot");
        self.body.push_str(&format!("  %{slot} = alloca i1\n"));

        let rhs_label = self.label("logic.rhs");
        let merge_label = self.label("logic.end");
        match expression.operator {
            Kind::AmpAmp => {
                self.body
                    .push_str(&format!("  store i1 false, ptr %{slot}\n"));
                self.body.push_str(&format!(
                    "  br i1 {}, label %{rhs_label}, label %{merge_label}\n",
                    left.repr
                ));
            }
            Kind::PipePipe => {
                self.body
                    .push_str(&format!("  store i1 true, ptr %{slot}\n"));
                self.body.push_str(&format!(
                    "  br i1 {}, label %{merge_label}, label %{rhs_label}\n",
                    left.repr
                ));
            }
            _ => unreachable!("logical expression checked by caller"),
        }

        self.start_block(&rhs_label);
        let right = self.emit_expression(&expression.right)?;
        if right.type_ != "bool" {
            return Err(CodegenError::unsupported("logical rhs type"));
        }
        self.body
            .push_str(&format!("  store i1 {}, ptr %{slot}\n", right.repr));
        self.body.push_str(&format!("  br label %{merge_label}\n"));

        self.start_block(&merge_label);
        let result = self.temp("logic");
        self.body
            .push_str(&format!("  %{result} = load i1, ptr %{slot}\n"));
        Ok(Value {
            type_: "bool".to_string(),
            repr: format!("%{result}"),
        })
    }

    fn emit_call(&mut self, call: &CallExpr) -> Result<Value, CodegenError> {
        if let Some(callee_type) = self.expression_type_hint(&call.callee)
            && let Some(function_type) = parse_function_type(&callee_type)
        {
            return self.emit_closure_call(call, &function_type);
        }

        if let Expression::Selector(selector) = &call.callee
            && let Some(value) = self.emit_enum_positional_constructor(selector, call)?
        {
            return Ok(value);
        }
        if let Expression::Selector(selector) = &call.callee {
            return self.emit_method_call(selector, call);
        }

        if let Expression::TypeApplication(applied) = &call.callee
            && let Expression::Ident(ident) = &applied.inner
            && ident.name == "chan_new"
        {
            return self.emit_chan_new(applied, call);
        }

        let Expression::Ident(callee) = &call.callee else {
            return Err(CodegenError::unsupported("non-identifier call"));
        };
        match callee.name.as_str() {
            "print" => self.emit_print(call),
            "panic" => self.emit_panic(call),
            "chr" => self.emit_chr(call),
            "i32_to_i64" => self.emit_i32_to_i64(call),
            "i64_to_i32" => self.emit_i64_to_i32(call),
            "sb_new" => self.emit_sb_new(call),
            "sb_write" => self.emit_sb_write(call),
            "sb_string" => self.emit_sb_string(call),
            "len" => self.emit_len(call),
            "append" => self.emit_append(call),
            "has" => self.emit_map_has(call),
            "delete" => self.emit_map_delete(call),
            "keys" => self.emit_map_keys(call),
            "chan_send" => self.emit_chan_send(call),
            "chan_recv" => self.emit_chan_recv(call),
            "chan_close" => self.emit_chan_close(call),
            "to_str" => self.emit_to_str(call),
            name => self.emit_user_call(name, call),
        }
    }

    fn emit_enum_positional_constructor(
        &mut self,
        selector: &SelectorExpr,
        call: &CallExpr,
    ) -> Result<Option<Value>, CodegenError> {
        let Expression::Ident(enum_ident) = &selector.inner else {
            return Ok(None);
        };
        if !self.generator.info.enums.contains_key(&enum_ident.name) {
            return Ok(None);
        }
        let enum_case = self.generator.enum_case(&enum_ident.name, &selector.name)?;
        if enum_case.fields.len() != 1 || call.args.len() != 1 {
            return Err(CodegenError::unsupported(format!(
                "enum positional constructor {}.{}",
                enum_ident.name, selector.name
            )));
        }
        let field_type = enum_case.fields[0].type_.clone();
        let field_value = self.emit_expression_as(&call.args[0], &field_type)?;
        if field_value.type_ != field_type {
            return Err(CodegenError::unsupported(format!(
                "enum constructor field has type {}",
                field_value.type_
            )));
        }
        let payload_type = enum_case.payload_type.clone();
        let payload = self.emit_struct_field_value(&payload_type, &field_value, 0)?;
        self.emit_enum_value(&enum_ident.name, &enum_case, Some(&payload))
            .map(Some)
    }

    fn emit_method_call(
        &mut self,
        selector: &SelectorExpr,
        call: &CallExpr,
    ) -> Result<Value, CodegenError> {
        let receiver = self.emit_expression(&selector.inner)?;
        if let Some(interface) = self.generator.info.interfaces.get(&receiver.type_).cloned() {
            let (method_index, method) = interface
                .methods
                .iter()
                .enumerate()
                .find(|(_, method)| method.name == selector.name)
                .map(|(idx, method)| (idx, method.clone()))
                .ok_or_else(|| {
                    CodegenError::unsupported(format!(
                        "unknown interface method {}.{}",
                        receiver.type_, selector.name
                    ))
                })?;
            if method.params.len() != call.args.len() {
                return Err(CodegenError::unsupported(format!(
                    "interface method call arity for {}.{}",
                    receiver.type_, selector.name
                )));
            }
            let mut args = Vec::new();
            for (idx, expression) in call.args.iter().enumerate() {
                args.push(self.emit_expression_as(expression, &method.params[idx])?);
            }
            return self.emit_interface_call(receiver, &method, method_index, args);
        }
        let signature = self
            .generator
            .method_signature(&receiver.type_, &selector.name)
            .ok_or_else(|| {
                CodegenError::unsupported(format!(
                    "unknown method {}.{}",
                    receiver.type_, selector.name
                ))
            })?;
        if signature.params.len() != call.args.len() + 1 {
            return Err(CodegenError::unsupported(format!(
                "method call arity for {}",
                signature.full_name
            )));
        }

        let mut args = vec![receiver];
        for (idx, expression) in call.args.iter().enumerate() {
            let expected = signature.params[idx + 1].as_str();
            let value = self.emit_expression_as(expression, expected)?;
            args.push(value);
        }
        self.emit_direct_call(&signature, args)
    }

    fn emit_interface_call(
        &mut self,
        receiver: Value,
        method: &InterfaceMethodInfo,
        method_index: usize,
        args: Vec<Value>,
    ) -> Result<Value, CodegenError> {
        let table = self.temp("iface.table");
        self.body.push_str(&format!(
            "  %{table} = extractvalue %yar.iface {}, 1\n",
            receiver.repr
        ));
        let is_nil = self.temp("iface.nil");
        let panic_label = self.label("iface.nil");
        let call_label = self.label("iface.call");
        self.body
            .push_str(&format!("  %{is_nil} = icmp eq ptr %{table}, null\n"));
        self.body.push_str(&format!(
            "  br i1 %{is_nil}, label %{panic_label}, label %{call_label}\n"
        ));
        self.terminated = true;

        self.start_block(&panic_label);
        self.terminated = false;
        self.emit_nil_interface_panic()?;

        self.start_block(&call_label);
        self.terminated = false;
        let data = self.temp("iface.data");
        self.body.push_str(&format!(
            "  %{data} = extractvalue %yar.iface {}, 0\n",
            receiver.repr
        ));
        let method_ptr_ptr = self.temp("iface.method.ptr");
        self.body.push_str(&format!(
            "  %{method_ptr_ptr} = getelementptr inbounds {}, ptr %{table}, i32 0, i32 {method_index}\n",
            interface_table_type_name(&receiver.type_)
        ));
        let method_ptr = self.temp("iface.method");
        self.body.push_str(&format!(
            "  %{method_ptr} = load ptr, ptr %{method_ptr_ptr}\n"
        ));

        let mut call_args = vec![format!("ptr %{data}")];
        for (idx, value) in args.into_iter().enumerate() {
            let expected = method.params[idx].as_str();
            if value.type_ != expected {
                return Err(CodegenError::unsupported(format!(
                    "interface argument {} has type {}",
                    idx + 1,
                    value.type_
                )));
            }
            call_args.push(format!("{} {}", self.llvm_type(expected)?, value.repr));
        }

        let return_type = if method.errorable {
            result_type_name(&method.return_type)
        } else {
            self.llvm_type(&method.return_type)?
        };
        if !method.errorable && matches!(method.return_type.as_str(), "void" | "noreturn") {
            self.body.push_str(&format!(
                "  call void %{method_ptr}({})\n",
                call_args.join(", ")
            ));
            if method.return_type == "noreturn" {
                self.body.push_str("  unreachable\n");
                self.terminated = true;
                return Ok(Value {
                    type_: "noreturn".to_string(),
                    repr: String::new(),
                });
            }
            return Ok(Value {
                type_: "void".to_string(),
                repr: String::new(),
            });
        }

        let result = self.temp("iface.call");
        self.body.push_str(&format!(
            "  %{result} = call {return_type} %{method_ptr}({})\n",
            call_args.join(", ")
        ));
        Ok(Value {
            type_: if method.errorable {
                errorable_type(&method.return_type)
            } else {
                method.return_type.clone()
            },
            repr: format!("%{result}"),
        })
    }

    fn emit_nil_interface_panic(&mut self) -> Result<(), CodegenError> {
        let message = self.emit_string_literal("nil interface method call")?;
        let ptr = self.temp("iface.panic.ptr");
        let len = self.temp("iface.panic.len");
        self.body.push_str(&format!(
            "  %{ptr} = extractvalue %yar.str {}, 0\n",
            message.repr
        ));
        self.body.push_str(&format!(
            "  %{len} = extractvalue %yar.str {}, 1\n",
            message.repr
        ));
        self.body
            .push_str(&format!("  call void @yar_panic(ptr %{ptr}, i64 %{len})\n"));
        self.body.push_str("  unreachable\n");
        self.terminated = true;
        Ok(())
    }

    fn emit_closure_call(
        &mut self,
        call: &CallExpr,
        function_type: &FunctionType,
    ) -> Result<Value, CodegenError> {
        if call.args.len() != function_type.params.len() {
            return Err(CodegenError::unsupported("function value call arity"));
        }
        let callee = self.emit_expression(&call.callee)?;
        let code = self.temp("closure.code");
        let env = self.temp("closure.env");
        self.body.push_str(&format!(
            "  %{code} = extractvalue %yar.closure {}, 0\n",
            callee.repr
        ));
        self.body.push_str(&format!(
            "  %{env} = extractvalue %yar.closure {}, 1\n",
            callee.repr
        ));

        let mut args = vec![format!("ptr %{env}")];
        for (idx, expression) in call.args.iter().enumerate() {
            let expected = &function_type.params[idx];
            let value = self.emit_expression_as(expression, expected)?;
            if value.type_ != *expected {
                return Err(CodegenError::unsupported(format!(
                    "function value argument {} has type {}",
                    idx + 1,
                    value.type_
                )));
            }
            args.push(format!("{} {}", self.llvm_type(expected)?, value.repr));
        }

        let return_type = if function_type.errorable {
            result_type_name(&function_type.return_type)
        } else {
            self.llvm_type(&function_type.return_type)?
        };
        if !function_type.errorable && function_type.return_type == "void" {
            self.body
                .push_str(&format!("  call void %{code}({})\n", args.join(", ")));
            return Ok(Value {
                type_: "void".to_string(),
                repr: String::new(),
            });
        }

        let result = self.temp("call");
        self.body.push_str(&format!(
            "  %{result} = call {return_type} %{code}({})\n",
            args.join(", ")
        ));
        Ok(Value {
            type_: if function_type.errorable {
                errorable_type(&function_type.return_type)
            } else {
                function_type.return_type.clone()
            },
            repr: format!("%{result}"),
        })
    }

    fn emit_print(&mut self, call: &CallExpr) -> Result<Value, CodegenError> {
        let [arg] = call.args.as_slice() else {
            return Err(CodegenError::unsupported("print arity"));
        };
        let arg = self.emit_expression(arg)?;
        if arg.type_ != "str" {
            return Err(CodegenError::unsupported("print non-str argument"));
        }
        let ptr = self.temp("print.ptr");
        let len = self.temp("print.len");
        self.body.push_str(&format!(
            "  %{ptr} = extractvalue %yar.str {}, 0\n",
            arg.repr
        ));
        self.body.push_str(&format!(
            "  %{len} = extractvalue %yar.str {}, 1\n",
            arg.repr
        ));
        self.body
            .push_str(&format!("  call void @yar_print(ptr %{ptr}, i64 %{len})\n"));
        Ok(Value {
            type_: "void".to_string(),
            repr: String::new(),
        })
    }

    fn emit_panic(&mut self, call: &CallExpr) -> Result<Value, CodegenError> {
        let [arg] = call.args.as_slice() else {
            return Err(CodegenError::unsupported("panic arity"));
        };
        let arg = self.emit_expression(arg)?;
        if arg.type_ != "str" {
            return Err(CodegenError::unsupported("panic non-str argument"));
        }
        let ptr = self.temp("panic.ptr");
        let len = self.temp("panic.len");
        self.body.push_str(&format!(
            "  %{ptr} = extractvalue %yar.str {}, 0\n",
            arg.repr
        ));
        self.body.push_str(&format!(
            "  %{len} = extractvalue %yar.str {}, 1\n",
            arg.repr
        ));
        self.body
            .push_str(&format!("  call void @yar_panic(ptr %{ptr}, i64 %{len})\n"));
        self.body.push_str("  unreachable\n");
        self.terminated = true;
        Ok(Value {
            type_: "noreturn".to_string(),
            repr: String::new(),
        })
    }

    fn emit_chr(&mut self, call: &CallExpr) -> Result<Value, CodegenError> {
        let [arg] = call.args.as_slice() else {
            return Err(CodegenError::unsupported("chr arity"));
        };
        let arg = self.emit_expression_as(arg, "i32")?;
        let result_slot = self.temp("chr.slot");
        self.body
            .push_str(&format!("  %{result_slot} = alloca %yar.str\n"));
        self.body.push_str(&format!(
            "  call void @yar_str_from_byte(i32 {}, ptr %{result_slot})\n",
            arg.repr
        ));
        let result = self.temp("chr");
        self.body.push_str(&format!(
            "  %{result} = load %yar.str, ptr %{result_slot}\n"
        ));
        Ok(Value {
            type_: "str".to_string(),
            repr: format!("%{result}"),
        })
    }

    fn emit_i32_to_i64(&mut self, call: &CallExpr) -> Result<Value, CodegenError> {
        let [arg] = call.args.as_slice() else {
            return Err(CodegenError::unsupported("i32_to_i64 arity"));
        };
        let arg = self.emit_expression_as(arg, "i32")?;
        let result = self.temp("widen");
        self.body
            .push_str(&format!("  %{result} = sext i32 {} to i64\n", arg.repr));
        Ok(Value {
            type_: "i64".to_string(),
            repr: format!("%{result}"),
        })
    }

    fn emit_i64_to_i32(&mut self, call: &CallExpr) -> Result<Value, CodegenError> {
        let [arg] = call.args.as_slice() else {
            return Err(CodegenError::unsupported("i64_to_i32 arity"));
        };
        let arg = self.emit_expression_as(arg, "i64")?;
        let result = self.temp("narrow");
        self.body
            .push_str(&format!("  %{result} = trunc i64 {} to i32\n", arg.repr));
        Ok(Value {
            type_: "i32".to_string(),
            repr: format!("%{result}"),
        })
    }

    fn emit_sb_new(&mut self, call: &CallExpr) -> Result<Value, CodegenError> {
        if !call.args.is_empty() {
            return Err(CodegenError::unsupported("sb_new arity"));
        }
        let handle = self.temp("sb.handle");
        self.body
            .push_str(&format!("  %{handle} = call i64 @yar_sb_new()\n"));
        Ok(Value {
            type_: "i64".to_string(),
            repr: format!("%{handle}"),
        })
    }

    fn emit_sb_write(&mut self, call: &CallExpr) -> Result<Value, CodegenError> {
        let [handle, data] = call.args.as_slice() else {
            return Err(CodegenError::unsupported("sb_write arity"));
        };
        let handle = self.emit_expression_as(handle, "i64")?;
        let data = self.emit_expression_as(data, "str")?;
        let data_ptr = self.temp("sb.str.ptr");
        let data_len = self.temp("sb.str.len");
        self.body.push_str(&format!(
            "  %{data_ptr} = extractvalue %yar.str {}, 0\n",
            data.repr
        ));
        self.body.push_str(&format!(
            "  %{data_len} = extractvalue %yar.str {}, 1\n",
            data.repr
        ));
        self.body.push_str(&format!(
            "  call void @yar_sb_write(i64 {}, ptr %{data_ptr}, i64 %{data_len})\n",
            handle.repr
        ));
        Ok(Value {
            type_: "void".to_string(),
            repr: String::new(),
        })
    }

    fn emit_sb_string(&mut self, call: &CallExpr) -> Result<Value, CodegenError> {
        let [handle] = call.args.as_slice() else {
            return Err(CodegenError::unsupported("sb_string arity"));
        };
        let handle = self.emit_expression_as(handle, "i64")?;
        let result_slot = self.temp("sb.result.slot");
        self.body
            .push_str(&format!("  %{result_slot} = alloca %yar.str\n"));
        self.body.push_str(&format!(
            "  call void @yar_sb_string(i64 {}, ptr %{result_slot})\n",
            handle.repr
        ));
        let result = self.temp("sb.result");
        self.body.push_str(&format!(
            "  %{result} = load %yar.str, ptr %{result_slot}\n"
        ));
        Ok(Value {
            type_: "str".to_string(),
            repr: format!("%{result}"),
        })
    }

    fn emit_len(&mut self, call: &CallExpr) -> Result<Value, CodegenError> {
        let [arg] = call.args.as_slice() else {
            return Err(CodegenError::unsupported("len arity"));
        };
        if let Some((len, _)) = self.static_array_len(arg)? {
            return Ok(Value {
                type_: "i32".to_string(),
                repr: len.to_string(),
            });
        }

        let arg = self.emit_expression(arg)?;
        if parse_map_type(&arg.type_).is_some() {
            let result = self.temp("map.len");
            self.body.push_str(&format!(
                "  %{result} = call i32 @yar_map_len(ptr {})\n",
                arg.repr
            ));
            return Ok(Value {
                type_: "i32".to_string(),
                repr: format!("%{result}"),
            });
        }
        if parse_slice_type(&arg.type_).is_some() {
            return self.extract_slice_field(&arg.repr, 1, "slice.len");
        }
        if arg.type_ == "str" {
            let len64 = self.temp("str.len64");
            self.body.push_str(&format!(
                "  %{len64} = extractvalue %yar.str {}, 1\n",
                arg.repr
            ));
            let len32 = self.temp("str.len");
            self.body
                .push_str(&format!("  %{len32} = trunc i64 %{len64} to i32\n"));
            return Ok(Value {
                type_: "i32".to_string(),
                repr: format!("%{len32}"),
            });
        }
        Err(CodegenError::unsupported("len unsupported argument"))
    }

    fn emit_map_has(&mut self, call: &CallExpr) -> Result<Value, CodegenError> {
        let [map_expr, key_expr] = call.args.as_slice() else {
            return Err(CodegenError::unsupported("has arity"));
        };
        let map = self.emit_expression(map_expr)?;
        let Some((key_type, _)) = parse_map_type(&map.type_) else {
            return Err(CodegenError::unsupported("has non-map argument"));
        };
        let key = self.emit_expression_as(key_expr, &key_type)?;
        let key_slot = self.emit_stack_slot("map.key.slot", &key_type, &key.repr)?;
        let raw = self.temp("map.has");
        self.body.push_str(&format!(
            "  %{raw} = call i32 @yar_map_has(ptr {}, ptr {key_slot})\n",
            map.repr
        ));
        let result = self.temp("map.has.bool");
        self.body
            .push_str(&format!("  %{result} = icmp ne i32 %{raw}, 0\n"));
        Ok(Value {
            type_: "bool".to_string(),
            repr: format!("%{result}"),
        })
    }

    fn emit_map_delete(&mut self, call: &CallExpr) -> Result<Value, CodegenError> {
        let [map_expr, key_expr] = call.args.as_slice() else {
            return Err(CodegenError::unsupported("delete arity"));
        };
        let map = self.emit_expression(map_expr)?;
        let Some((key_type, _)) = parse_map_type(&map.type_) else {
            return Err(CodegenError::unsupported("delete non-map argument"));
        };
        let key = self.emit_expression_as(key_expr, &key_type)?;
        let key_slot = self.emit_stack_slot("map.key.slot", &key_type, &key.repr)?;
        self.body.push_str(&format!(
            "  call void @yar_map_delete(ptr {}, ptr {key_slot})\n",
            map.repr
        ));
        Ok(Value {
            type_: "void".to_string(),
            repr: String::new(),
        })
    }

    fn emit_map_keys(&mut self, call: &CallExpr) -> Result<Value, CodegenError> {
        let [map_expr] = call.args.as_slice() else {
            return Err(CodegenError::unsupported("keys arity"));
        };
        let map = self.emit_expression(map_expr)?;
        let Some((key_type, _)) = parse_map_type(&map.type_) else {
            return Err(CodegenError::unsupported("keys non-map argument"));
        };
        let result_slot = self.temp("map.keys.slot");
        self.body
            .push_str(&format!("  %{result_slot} = alloca %yar.slice\n"));
        self.body.push_str(&format!(
            "  call void @yar_map_keys(ptr {}, ptr %{result_slot})\n",
            map.repr
        ));
        let result = self.temp("map.keys");
        self.body.push_str(&format!(
            "  %{result} = load %yar.slice, ptr %{result_slot}\n"
        ));
        Ok(Value {
            type_: format!("[]{key_type}"),
            repr: format!("%{result}"),
        })
    }

    fn emit_chan_new(
        &mut self,
        applied: &TypeApplicationExpr,
        call: &CallExpr,
    ) -> Result<Value, CodegenError> {
        let [capacity_expr] = call.args.as_slice() else {
            return Err(CodegenError::unsupported("chan_new arity"));
        };
        let [elem_ref] = applied.type_args.as_slice() else {
            return Err(CodegenError::unsupported("chan_new type argument count"));
        };
        let element_type = elem_ref.to_string();
        let capacity = self.emit_expression_as(capacity_expr, "i32")?;
        if capacity.type_ != "i32" {
            return Err(CodegenError::unsupported(format!(
                "chan_new capacity has type {}",
                capacity.type_
            )));
        }
        let elem_size = self.type_size(&element_type)?;
        let handle = self.temp("chan.new");
        self.body.push_str(&format!(
            "  %{handle} = call ptr @yar_chan_new(i64 {elem_size}, i32 {})\n",
            capacity.repr
        ));
        Ok(Value {
            type_: format!("chan[{element_type}]"),
            repr: format!("%{handle}"),
        })
    }

    fn emit_chan_send(&mut self, call: &CallExpr) -> Result<Value, CodegenError> {
        let [channel_expr, value_expr] = call.args.as_slice() else {
            return Err(CodegenError::unsupported("chan_send arity"));
        };
        let channel = self.emit_expression(channel_expr)?;
        let Some(element_type) = parse_chan_type(&channel.type_) else {
            return Err(CodegenError::unsupported("chan_send non-channel argument"));
        };
        let value = self.emit_expression_as(value_expr, &element_type)?;
        if value.type_ != element_type {
            return Err(CodegenError::unsupported(format!(
                "chan_send value has type {}",
                value.type_
            )));
        }
        let slot = self.emit_stack_slot("chan.send.slot", &element_type, &value.repr)?;
        let status = self.temp("chan.send.status");
        self.body.push_str(&format!(
            "  %{status} = call i32 @yar_chan_send(ptr {}, ptr {slot})\n",
            channel.repr
        ));
        self.emit_closed_status_result("void", &format!("%{status}"), None)
    }

    fn emit_chan_recv(&mut self, call: &CallExpr) -> Result<Value, CodegenError> {
        let [channel_expr] = call.args.as_slice() else {
            return Err(CodegenError::unsupported("chan_recv arity"));
        };
        let channel = self.emit_expression(channel_expr)?;
        let Some(element_type) = parse_chan_type(&channel.type_) else {
            return Err(CodegenError::unsupported("chan_recv non-channel argument"));
        };
        let out = self.temp("chan.recv.out");
        let element_llvm = self.llvm_type(&element_type)?;
        let zero = self.zero_value(&element_type)?;
        self.body
            .push_str(&format!("  %{out} = alloca {element_llvm}\n"));
        self.body.push_str(&format!(
            "  store {element_llvm} {}, ptr %{out}\n",
            zero.repr
        ));
        let status = self.temp("chan.recv.status");
        self.body.push_str(&format!(
            "  %{status} = call i32 @yar_chan_recv(ptr {}, ptr %{out})\n",
            channel.repr
        ));
        let value = self.load_address(&Address {
            type_: element_type.clone(),
            ptr: format!("%{out}"),
        })?;
        self.emit_closed_status_result(&element_type, &format!("%{status}"), Some(&value.repr))
    }

    fn emit_chan_close(&mut self, call: &CallExpr) -> Result<Value, CodegenError> {
        let [channel_expr] = call.args.as_slice() else {
            return Err(CodegenError::unsupported("chan_close arity"));
        };
        let channel = self.emit_expression(channel_expr)?;
        if parse_chan_type(&channel.type_).is_none() {
            return Err(CodegenError::unsupported("chan_close non-channel argument"));
        }
        self.body.push_str(&format!(
            "  call void @yar_chan_close(ptr {})\n",
            channel.repr
        ));
        Ok(Value {
            type_: "void".to_string(),
            repr: String::new(),
        })
    }

    fn emit_map_lookup(
        &mut self,
        expression: &IndexExpr,
        key_type: &str,
        value_type: &str,
    ) -> Result<Value, CodegenError> {
        let map = self.emit_expression(&expression.inner)?;
        let key = self.emit_expression_as(&expression.index, key_type)?;
        let key_slot = self.emit_stack_slot("map.key.slot", key_type, &key.repr)?;
        let value_slot = self.temp("map.val.slot");
        self.body.push_str(&format!(
            "  %{value_slot} = alloca {}\n",
            self.llvm_type(value_type)?
        ));

        let found = self.temp("map.found");
        self.body.push_str(&format!(
            "  %{found} = call i32 @yar_map_get(ptr {}, ptr {key_slot}, ptr %{value_slot})\n",
            map.repr
        ));
        let is_found = self.temp("map.is_found");
        self.body
            .push_str(&format!("  %{is_found} = icmp ne i32 %{found}, 0\n"));

        let ok_label = self.label("map.ok");
        let miss_label = self.label("map.miss");
        let end_label = self.label("map.end");
        self.body.push_str(&format!(
            "  br i1 %{is_found}, label %{ok_label}, label %{miss_label}\n"
        ));

        self.start_block(&ok_label);
        let ok_value = self.temp("map.val");
        self.body.push_str(&format!(
            "  %{ok_value} = load {}, ptr %{value_slot}\n",
            self.llvm_type(value_type)?
        ));
        let ok_result = self.emit_success_result(value_type, &format!("%{ok_value}"))?;
        self.body.push_str(&format!("  br label %{end_label}\n"));
        let ok_block = self.current_block.clone();

        self.start_block(&miss_label);
        let code = self.generator.error_code("error.MissingKey")?;
        let miss_result = self.emit_error_code_result(value_type, &code.to_string())?;
        self.body.push_str(&format!("  br label %{end_label}\n"));
        let miss_block = self.current_block.clone();

        self.start_block(&end_label);
        let result = self.temp("map.result");
        let result_type = result_type_name(value_type);
        self.body.push_str(&format!(
            "  %{result} = phi {result_type} [{}, %{ok_block}], [{}, %{miss_block}]\n",
            ok_result.repr, miss_result.repr
        ));
        Ok(Value {
            type_: errorable_type(value_type),
            repr: format!("%{result}"),
        })
    }

    fn emit_append(&mut self, call: &CallExpr) -> Result<Value, CodegenError> {
        let [slice_expr, element_expr] = call.args.as_slice() else {
            return Err(CodegenError::unsupported("append arity"));
        };
        let slice = self.emit_expression(slice_expr)?;
        let Some(element_type) = parse_slice_type(&slice.type_) else {
            return Err(CodegenError::unsupported("append non-slice argument"));
        };
        let element = self.emit_expression_as(element_expr, &element_type)?;
        if element.type_ != element_type {
            return Err(CodegenError::unsupported(format!(
                "append element has type {}",
                element.type_
            )));
        }

        let old_data = self.extract_slice_field(&slice.repr, 0, "append.data")?;
        let old_len = self.extract_slice_field(&slice.repr, 1, "append.len")?;
        let old_cap = self.extract_slice_field(&slice.repr, 2, "append.cap")?;
        let new_len = self.temp("append.len");
        self.body
            .push_str(&format!("  %{new_len} = add i32 {}, 1\n", old_len.repr));
        let old_len64 = self.int_to_i64(&old_len)?;

        let needs_grow = self.temp("append.needs_grow");
        let grow_label = self.label("append.grow");
        let reuse_label = self.label("append.reuse");
        let write_label = self.label("append.write");
        self.body.push_str(&format!(
            "  %{needs_grow} = icmp eq i32 {}, {}\n",
            old_len.repr, old_cap.repr
        ));
        self.body.push_str(&format!(
            "  br i1 %{needs_grow}, label %{grow_label}, label %{reuse_label}\n"
        ));

        self.start_block(&reuse_label);
        self.body.push_str(&format!("  br label %{write_label}\n"));

        self.start_block(&grow_label);
        let cap_was_zero = self.temp("append.cap_zero");
        let doubled_cap = self.temp("append.cap_double");
        let new_cap = self.temp("append.cap");
        self.body.push_str(&format!(
            "  %{cap_was_zero} = icmp eq i32 {}, 0\n",
            old_cap.repr
        ));
        self.body
            .push_str(&format!("  %{doubled_cap} = mul i32 {}, 2\n", old_cap.repr));
        self.body.push_str(&format!(
            "  %{new_cap} = select i1 %{cap_was_zero}, i32 1, i32 %{doubled_cap}\n"
        ));
        let new_cap64 = self.int_to_i64(&Value {
            type_: "i32".to_string(),
            repr: format!("%{new_cap}"),
        })?;
        let alloc_size = self.emit_scaled_size(&element_type, &new_cap64)?;
        let new_data = self.emit_alloc_bytes(&alloc_size, false);
        let has_existing = self.temp("append.has_existing");
        let copy_label = self.label("append.copy");
        let ready_label = self.label("append.ready");
        self.body.push_str(&format!(
            "  %{has_existing} = icmp ne i32 {}, 0\n",
            old_len.repr
        ));
        self.body.push_str(&format!(
            "  br i1 %{has_existing}, label %{copy_label}, label %{ready_label}\n"
        ));

        self.start_block(&copy_label);
        let copy_size = self.emit_scaled_size(&element_type, &old_len64)?;
        self.emit_memcpy(&new_data, &old_data.repr, &copy_size);
        self.body.push_str(&format!("  br label %{ready_label}\n"));

        self.start_block(&ready_label);
        self.body.push_str(&format!("  br label %{write_label}\n"));

        self.start_block(&write_label);
        let data_phi = self.temp("append.data");
        let cap_phi = self.temp("append.cap");
        self.body.push_str(&format!(
            "  %{data_phi} = phi ptr [{}, %{reuse_label}], [{new_data}, %{ready_label}]\n",
            old_data.repr
        ));
        self.body.push_str(&format!(
            "  %{cap_phi} = phi i32 [{}, %{reuse_label}], [%{new_cap}, %{ready_label}]\n",
            old_cap.repr
        ));
        let element_ptr = self.temp("append.elem.ptr");
        self.body.push_str(&format!(
            "  %{element_ptr} = getelementptr {}, ptr %{data_phi}, i64 {old_len64}\n",
            self.llvm_type(&element_type)?
        ));
        self.body.push_str(&format!(
            "  store {} {}, ptr %{element_ptr}\n",
            self.llvm_type(&element_type)?,
            element.repr
        ));

        Ok(Value {
            type_: slice.type_,
            repr: self.emit_slice_value(
                &format!("%{data_phi}"),
                &format!("%{new_len}"),
                &format!("%{cap_phi}"),
            ),
        })
    }

    fn static_array_len(
        &self,
        expression: &Expression,
    ) -> Result<Option<(usize, String)>, CodegenError> {
        match expression {
            Expression::Ident(expr) => {
                let Some(local) = self.locals.get(&expr.name) else {
                    return Err(CodegenError::unsupported(format!(
                        "unknown local {:?}",
                        expr.name
                    )));
                };
                Ok(parse_array_type(&local.type_))
            }
            Expression::ArrayLiteral(expr) => Ok(parse_array_type(&expr.type_ref.to_string())),
            Expression::Group(expr) => self.static_array_len(&expr.inner),
            _ => Ok(None),
        }
    }

    fn emit_to_str(&mut self, call: &CallExpr) -> Result<Value, CodegenError> {
        let [arg] = call.args.as_slice() else {
            return Err(CodegenError::unsupported("to_str arity"));
        };
        let arg = self.emit_expression(arg)?;
        match arg.type_.as_str() {
            "i32" => {
                let result_slot = self.temp("to_str.slot");
                self.body
                    .push_str(&format!("  %{result_slot} = alloca %yar.str\n"));
                self.body.push_str(&format!(
                    "  call void @yar_to_str_i32(i32 {}, ptr %{result_slot})\n",
                    arg.repr
                ));
                let result = self.temp("to_str");
                self.body.push_str(&format!(
                    "  %{result} = load %yar.str, ptr %{result_slot}\n"
                ));
                Ok(Value {
                    type_: "str".to_string(),
                    repr: format!("%{result}"),
                })
            }
            "i64" => {
                let result_slot = self.temp("to_str.slot");
                self.body
                    .push_str(&format!("  %{result_slot} = alloca %yar.str\n"));
                self.body.push_str(&format!(
                    "  call void @yar_to_str_i64(i64 {}, ptr %{result_slot})\n",
                    arg.repr
                ));
                let result = self.temp("to_str");
                self.body.push_str(&format!(
                    "  %{result} = load %yar.str, ptr %{result_slot}\n"
                ));
                Ok(Value {
                    type_: "str".to_string(),
                    repr: format!("%{result}"),
                })
            }
            "bool" => {
                let true_value = self.emit_string_literal("true")?;
                let false_value = self.emit_string_literal("false")?;
                let result = self.temp("to_str");
                self.body.push_str(&format!(
                    "  %{result} = select i1 {}, %yar.str {}, %yar.str {}\n",
                    arg.repr, true_value.repr, false_value.repr
                ));
                Ok(Value {
                    type_: "str".to_string(),
                    repr: format!("%{result}"),
                })
            }
            "str" => Ok(arg),
            "error" => self.emit_error_to_str(&arg.repr),
            _ => Err(CodegenError::unsupported(format!(
                "to_str argument {}",
                arg.type_
            ))),
        }
    }

    fn emit_error_to_str(&mut self, code: &str) -> Result<Value, CodegenError> {
        let result_ptr = self.temp("to_str.err.ptr");
        self.body
            .push_str(&format!("  %{result_ptr} = alloca %yar.str\n"));
        let end_label = self.label("to_str.err.end");
        let default_label = self.label("to_str.err.default");
        let labels = self
            .generator
            .info
            .ordered_errors
            .iter()
            .map(|name| {
                self.generator
                    .error_code(name)
                    .map(|code| self.label(&format!("to_str.err.{code}")))
            })
            .collect::<Result<Vec<_>, _>>()?;

        self.body
            .push_str(&format!("  switch i32 {code}, label %{default_label} [\n"));
        for (idx, name) in self.generator.info.ordered_errors.iter().enumerate() {
            let error_code = self.generator.error_code(name)?;
            self.body
                .push_str(&format!("    i32 {error_code}, label %{}\n", labels[idx]));
        }
        self.body.push_str("  ]\n");

        for (idx, name) in self
            .generator
            .info
            .ordered_errors
            .clone()
            .iter()
            .enumerate()
        {
            self.start_block(&labels[idx]);
            self.terminated = false;
            let display_name = self
                .generator
                .info
                .error_display_names
                .get(name)
                .ok_or_else(|| {
                    CodegenError::unsupported(format!("missing error display name {name:?}"))
                })?;
            let value = self.emit_string_literal(&format!("error.{display_name}"))?;
            self.body.push_str(&format!(
                "  store %yar.str {}, ptr %{result_ptr}\n",
                value.repr
            ));
            self.body.push_str(&format!("  br label %{end_label}\n"));
        }

        self.start_block(&default_label);
        self.terminated = false;
        let unknown = self.emit_string_literal("error.unknown")?;
        self.body.push_str(&format!(
            "  store %yar.str {}, ptr %{result_ptr}\n",
            unknown.repr
        ));
        self.body.push_str(&format!("  br label %{end_label}\n"));

        self.start_block(&end_label);
        self.terminated = false;
        let result = self.temp("to_str.err.result");
        self.body
            .push_str(&format!("  %{result} = load %yar.str, ptr %{result_ptr}\n"));
        Ok(Value {
            type_: "str".to_string(),
            repr: format!("%{result}"),
        })
    }

    fn emit_user_call(&mut self, name: &str, call: &CallExpr) -> Result<Value, CodegenError> {
        let signature = self
            .generator
            .info
            .functions
            .get(name)
            .ok_or_else(|| CodegenError::unsupported(format!("unknown function {name:?}")))?
            .clone();
        if signature.params.len() != call.args.len() {
            return Err(CodegenError::unsupported(format!(
                "call arity for {name:?}"
            )));
        }
        let mut args = Vec::new();
        for (idx, expression) in call.args.iter().enumerate() {
            let expected = signature.params[idx].as_str();
            let value = self.emit_expression_as(expression, expected)?;
            args.push(value);
        }

        if signature.host_intrinsic {
            return self.emit_host_intrinsic_call(&signature, args);
        }
        self.emit_direct_call(&signature, args)
    }

    fn emit_host_intrinsic_call(
        &mut self,
        signature: &Signature,
        args: Vec<Value>,
    ) -> Result<Value, CodegenError> {
        match signature.full_name.as_str() {
            "fs.read_file" => {
                let path = self.emit_str_abi_input("fs.read_file.path", &args[0]);
                let out = self.temp("fs.read_file.out");
                self.body.push_str(&format!("  %{out} = alloca %yar.str\n"));
                self.body
                    .push_str(&format!("  store %yar.str zeroinitializer, ptr %{out}\n"));
                let status = self.temp("fs.read_file.status");
                self.body.push_str(&format!(
                    "  %{status} = call i32 @yar_fs_read_file(ptr {path}, ptr %{out})\n"
                ));
                let value = self.temp("fs.read_file.value");
                self.body
                    .push_str(&format!("  %{value} = load %yar.str, ptr %{out}\n"));
                self.emit_host_status_result(
                    &signature.full_name,
                    &signature.return_type,
                    &format!("%{status}"),
                    Some(&format!("%{value}")),
                )
            }
            "fs.write_file" => {
                let path = self.emit_str_abi_input("fs.write_file.path", &args[0]);
                let data = self.emit_str_abi_input("fs.write_file.data", &args[1]);
                let status = self.temp("fs.write_file.status");
                self.body.push_str(&format!(
                    "  %{status} = call i32 @yar_fs_write_file(ptr {path}, ptr {data})\n"
                ));
                self.emit_host_status_result(
                    &signature.full_name,
                    &signature.return_type,
                    &format!("%{status}"),
                    None,
                )
            }
            "fs.read_dir" => {
                let path = self.emit_str_abi_input("fs.read_dir.path", &args[0]);
                let out = self.temp("fs.read_dir.out");
                self.body
                    .push_str(&format!("  %{out} = alloca %yar.slice\n"));
                self.body
                    .push_str(&format!("  store %yar.slice zeroinitializer, ptr %{out}\n"));
                let status = self.temp("fs.read_dir.status");
                self.body.push_str(&format!(
                    "  %{status} = call i32 @yar_fs_read_dir(ptr {path}, ptr %{out})\n"
                ));
                let value = self.temp("fs.read_dir.value");
                self.body
                    .push_str(&format!("  %{value} = load %yar.slice, ptr %{out}\n"));
                self.emit_host_status_result(
                    &signature.full_name,
                    &signature.return_type,
                    &format!("%{status}"),
                    Some(&format!("%{value}")),
                )
            }
            "fs.stat" => {
                let path = self.emit_str_abi_input("fs.stat.path", &args[0]);
                let out = self.temp("fs.stat.out");
                self.body.push_str(&format!("  %{out} = alloca i32\n"));
                self.body.push_str(&format!("  store i32 0, ptr %{out}\n"));
                let status = self.temp("fs.stat.status");
                self.body.push_str(&format!(
                    "  %{status} = call i32 @yar_fs_stat(ptr {path}, ptr %{out})\n"
                ));
                let tag = self.temp("fs.stat.tag");
                self.body
                    .push_str(&format!("  %{tag} = load i32, ptr %{out}\n"));
                let value = self.emit_enum_tag_value(&signature.return_type, &format!("%{tag}"))?;
                self.emit_host_status_result(
                    &signature.full_name,
                    &signature.return_type,
                    &format!("%{status}"),
                    Some(&value.repr),
                )
            }
            "fs.mkdir_all" => {
                let path = self.emit_str_abi_input("fs.mkdir_all.path", &args[0]);
                let status = self.temp("fs.mkdir_all.status");
                self.body.push_str(&format!(
                    "  %{status} = call i32 @yar_fs_mkdir_all(ptr {path})\n"
                ));
                self.emit_host_status_result(
                    &signature.full_name,
                    &signature.return_type,
                    &format!("%{status}"),
                    None,
                )
            }
            "fs.remove_all" => {
                let path = self.emit_str_abi_input("fs.remove_all.path", &args[0]);
                let status = self.temp("fs.remove_all.status");
                self.body.push_str(&format!(
                    "  %{status} = call i32 @yar_fs_remove_all(ptr {path})\n"
                ));
                self.emit_host_status_result(
                    &signature.full_name,
                    &signature.return_type,
                    &format!("%{status}"),
                    None,
                )
            }
            "fs.temp_dir" => {
                let prefix = self.emit_str_abi_input("fs.temp_dir.prefix", &args[0]);
                let out = self.temp("fs.temp_dir.out");
                self.body.push_str(&format!("  %{out} = alloca %yar.str\n"));
                self.body
                    .push_str(&format!("  store %yar.str zeroinitializer, ptr %{out}\n"));
                let status = self.temp("fs.temp_dir.status");
                self.body.push_str(&format!(
                    "  %{status} = call i32 @yar_fs_temp_dir(ptr {prefix}, ptr %{out})\n"
                ));
                let value = self.temp("fs.temp_dir.value");
                self.body
                    .push_str(&format!("  %{value} = load %yar.str, ptr %{out}\n"));
                self.emit_host_status_result(
                    &signature.full_name,
                    &signature.return_type,
                    &format!("%{status}"),
                    Some(&format!("%{value}")),
                )
            }
            "fs.open_read_handle" => self.emit_fs_i64_out_call(
                signature,
                "fs.open_read_handle",
                "yar_fs_open_read",
                args,
            ),
            "fs.open_write_handle" => self.emit_fs_i64_out_call(
                signature,
                "fs.open_write_handle",
                "yar_fs_open_write",
                args,
            ),
            "fs.read_handle" => {
                let out = self.temp("fs.read_handle.out");
                self.body.push_str(&format!("  %{out} = alloca %yar.str\n"));
                self.body
                    .push_str(&format!("  store %yar.str zeroinitializer, ptr %{out}\n"));
                let status = self.temp("fs.read_handle.status");
                self.body.push_str(&format!(
                    "  %{status} = call i32 @yar_fs_read_handle(i64 {}, i32 {}, ptr %{out})\n",
                    args[0].repr, args[1].repr
                ));
                let value = self.temp("fs.read_handle.value");
                self.body
                    .push_str(&format!("  %{value} = load %yar.str, ptr %{out}\n"));
                self.emit_host_status_result(
                    &signature.full_name,
                    &signature.return_type,
                    &format!("%{status}"),
                    Some(&format!("%{value}")),
                )
            }
            "fs.write_handle" => {
                let data = self.emit_str_abi_input("fs.write_handle.data", &args[1]);
                let out = self.temp("fs.write_handle.out");
                self.body.push_str(&format!("  %{out} = alloca i32\n"));
                self.body.push_str(&format!("  store i32 0, ptr %{out}\n"));
                let status = self.temp("fs.write_handle.status");
                self.body.push_str(&format!(
                    "  %{status} = call i32 @yar_fs_write_handle(i64 {}, ptr {data}, ptr %{out})\n",
                    args[0].repr
                ));
                let value = self.temp("fs.write_handle.value");
                self.body
                    .push_str(&format!("  %{value} = load i32, ptr %{out}\n"));
                self.emit_host_status_result(
                    &signature.full_name,
                    &signature.return_type,
                    &format!("%{status}"),
                    Some(&format!("%{value}")),
                )
            }
            "fs.close_handle" => {
                let status = self.temp("fs.close_handle.status");
                self.body.push_str(&format!(
                    "  %{status} = call i32 @yar_fs_close_handle(i64 {})\n",
                    args[0].repr
                ));
                self.emit_host_status_result(
                    &signature.full_name,
                    &signature.return_type,
                    &format!("%{status}"),
                    None,
                )
            }
            "process.args" => {
                let out = self.temp("process.args.out");
                self.body
                    .push_str(&format!("  %{out} = alloca %yar.slice\n"));
                self.body
                    .push_str(&format!("  store %yar.slice zeroinitializer, ptr %{out}\n"));
                self.body
                    .push_str(&format!("  call void @yar_process_args(ptr %{out})\n"));
                let value = self.temp("process.args.value");
                self.body
                    .push_str(&format!("  %{value} = load %yar.slice, ptr %{out}\n"));
                Ok(Value {
                    type_: signature.return_type.clone(),
                    repr: format!("%{value}"),
                })
            }
            "process.run" => {
                let argv = self.temp("process.run.argv");
                self.body
                    .push_str(&format!("  %{argv} = alloca %yar.slice\n"));
                self.body.push_str(&format!(
                    "  store %yar.slice {}, ptr %{argv}\n",
                    args[0].repr
                ));
                let timeout = self.emit_struct_field_extract(
                    "process.run.timeout",
                    &args[1],
                    "timeout_milliseconds",
                )?;
                let max_stdout = self.emit_struct_field_extract(
                    "process.run.max_stdout",
                    &args[1],
                    "max_stdout_bytes",
                )?;
                let max_stderr = self.emit_struct_field_extract(
                    "process.run.max_stderr",
                    &args[1],
                    "max_stderr_bytes",
                )?;
                let cancellation =
                    self.emit_struct_field_extract("process.run.cancellation", &args[2], "signal")?;
                let out = self.temp("process.run.out");
                let return_type = self.llvm_type(&signature.return_type)?.to_string();
                self.body
                    .push_str(&format!("  %{out} = alloca {return_type}\n"));
                self.body.push_str(&format!(
                    "  store {return_type} zeroinitializer, ptr %{out}\n"
                ));
                let status = self.temp("process.run.status");
                self.body.push_str(&format!(
                    "  %{status} = call i32 @yar_process_run(ptr %{argv}, i64 {}, i64 {}, i64 {}, ptr {}, ptr %{out})\n",
                    timeout.repr, max_stdout.repr, max_stderr.repr, cancellation.repr
                ));
                let value = self.temp("process.run.value");
                self.body
                    .push_str(&format!("  %{value} = load {return_type}, ptr %{out}\n"));
                self.emit_host_status_result(
                    &signature.full_name,
                    &signature.return_type,
                    &format!("%{status}"),
                    Some(&format!("%{value}")),
                )
            }
            "process.run_inherit" => {
                let argv = self.temp("process.run_inherit.argv");
                self.body
                    .push_str(&format!("  %{argv} = alloca %yar.slice\n"));
                self.body.push_str(&format!(
                    "  store %yar.slice {}, ptr %{argv}\n",
                    args[0].repr
                ));
                let out = self.temp("process.run_inherit.out");
                self.body.push_str(&format!("  %{out} = alloca i32\n"));
                self.body.push_str(&format!("  store i32 0, ptr %{out}\n"));
                let status = self.temp("process.run_inherit.status");
                let cancellation = self.emit_struct_field_extract(
                    "process.run_inherit.cancellation",
                    &args[2],
                    "signal",
                )?;
                self.body.push_str(&format!(
                    "  %{status} = call i32 @yar_process_run_inherit(ptr %{argv}, i64 {}, ptr {}, ptr %{out})\n",
                    args[1].repr, cancellation.repr
                ));
                let value = self.temp("process.run_inherit.value");
                self.body
                    .push_str(&format!("  %{value} = load i32, ptr %{out}\n"));
                self.emit_host_status_result(
                    &signature.full_name,
                    &signature.return_type,
                    &format!("%{status}"),
                    Some(&format!("%{value}")),
                )
            }
            "env.lookup" => {
                let name = self.emit_str_abi_input("env.lookup.name", &args[0]);
                let out = self.temp("env.lookup.out");
                self.body.push_str(&format!("  %{out} = alloca %yar.str\n"));
                self.body
                    .push_str(&format!("  store %yar.str zeroinitializer, ptr %{out}\n"));
                let status = self.temp("env.lookup.status");
                self.body.push_str(&format!(
                    "  %{status} = call i32 @yar_env_lookup(ptr {name}, ptr %{out})\n"
                ));
                let value = self.temp("env.lookup.value");
                self.body
                    .push_str(&format!("  %{value} = load %yar.str, ptr %{out}\n"));
                self.emit_host_status_result(
                    &signature.full_name,
                    &signature.return_type,
                    &format!("%{status}"),
                    Some(&format!("%{value}")),
                )
            }
            "stdio.eprint" => {
                let ptr = self.temp("eprint.ptr");
                let len = self.temp("eprint.len");
                self.body.push_str(&format!(
                    "  %{ptr} = extractvalue %yar.str {}, 0\n",
                    args[0].repr
                ));
                self.body.push_str(&format!(
                    "  %{len} = extractvalue %yar.str {}, 1\n",
                    args[0].repr
                ));
                self.body.push_str(&format!(
                    "  call void @yar_eprint(ptr %{ptr}, i64 %{len})\n"
                ));
                Ok(Value {
                    type_: "void".to_string(),
                    repr: String::new(),
                })
            }
            "net.listen" => {
                let host = self.emit_str_abi_input("net.listen.host", &args[0]);
                self.emit_host_out_call(
                    signature,
                    "net.listen",
                    "i64",
                    "yar_net_listen",
                    format!("ptr {host}, i32 {}", args[1].repr),
                )
            }
            "net.accept" => self.emit_host_out_call(
                signature,
                "net.accept",
                "i64",
                "yar_net_accept",
                format!("i64 {}", args[0].repr),
            ),
            "net.listener_addr" => {
                let out_type = self.llvm_type(&signature.return_type)?.to_string();
                self.emit_host_out_call(
                    signature,
                    "net.listener_addr",
                    &out_type,
                    "yar_net_listener_addr",
                    format!("i64 {}", args[0].repr),
                )
            }
            "net.close_listener" => self.emit_host_status_call(
                signature,
                "net.close_listener",
                "yar_net_close_listener",
                format!("i64 {}", args[0].repr),
            ),
            "net.connect" => {
                let host = self.emit_str_abi_input("net.connect.host", &args[0]);
                self.emit_host_out_call(
                    signature,
                    "net.connect",
                    "i64",
                    "yar_net_connect",
                    format!("ptr {host}, i32 {}", args[1].repr),
                )
            }
            "net.read" => self.emit_host_out_call(
                signature,
                "net.read",
                "%yar.str",
                "yar_net_read",
                format!("i64 {}, i32 {}", args[0].repr, args[1].repr),
            ),
            "net.write" => {
                let data = self.emit_str_abi_input("net.write.data", &args[1]);
                self.emit_host_out_call(
                    signature,
                    "net.write",
                    "i32",
                    "yar_net_write",
                    format!("i64 {}, ptr {data}", args[0].repr),
                )
            }
            "net.close" => self.emit_host_status_call(
                signature,
                "net.close",
                "yar_net_close",
                format!("i64 {}", args[0].repr),
            ),
            "net.local_addr" => {
                let out_type = self.llvm_type(&signature.return_type)?.to_string();
                self.emit_host_out_call(
                    signature,
                    "net.local_addr",
                    &out_type,
                    "yar_net_local_addr",
                    format!("i64 {}", args[0].repr),
                )
            }
            "net.remote_addr" => {
                let out_type = self.llvm_type(&signature.return_type)?.to_string();
                self.emit_host_out_call(
                    signature,
                    "net.remote_addr",
                    &out_type,
                    "yar_net_remote_addr",
                    format!("i64 {}", args[0].repr),
                )
            }
            "net.set_read_deadline" => self.emit_host_status_call(
                signature,
                "net.set_read_deadline",
                "yar_net_set_read_deadline",
                format!("i64 {}, i32 {}", args[0].repr, args[1].repr),
            ),
            "net.set_write_deadline" => self.emit_host_status_call(
                signature,
                "net.set_write_deadline",
                "yar_net_set_write_deadline",
                format!("i64 {}, i32 {}", args[0].repr, args[1].repr),
            ),
            "net.resolve" => {
                let host = self.emit_str_abi_input("net.resolve.host", &args[0]);
                let out_type = self.llvm_type(&signature.return_type)?.to_string();
                self.emit_host_out_call(
                    signature,
                    "net.resolve",
                    &out_type,
                    "yar_net_resolve",
                    format!("ptr {host}, i32 {}", args[1].repr),
                )
            }
            _ => Err(CodegenError::unsupported(format!(
                "host intrinsic {:?}",
                signature.full_name
            ))),
        }
    }

    fn emit_host_out_call(
        &mut self,
        signature: &Signature,
        temp_prefix: &str,
        out_type: &str,
        runtime_name: &str,
        call_args: String,
    ) -> Result<Value, CodegenError> {
        let out = self.temp(&format!("{temp_prefix}.out"));
        self.body
            .push_str(&format!("  %{out} = alloca {out_type}\n"));
        self.body
            .push_str(&format!("  store {out_type} zeroinitializer, ptr %{out}\n"));
        let status = self.temp(&format!("{temp_prefix}.status"));
        self.body.push_str(&format!(
            "  %{status} = call i32 @{runtime_name}({call_args}, ptr %{out})\n"
        ));
        let value = self.temp(&format!("{temp_prefix}.value"));
        self.body
            .push_str(&format!("  %{value} = load {out_type}, ptr %{out}\n"));
        self.emit_host_status_result(
            &signature.full_name,
            &signature.return_type,
            &format!("%{status}"),
            Some(&format!("%{value}")),
        )
    }

    fn emit_str_abi_input(&mut self, temp_prefix: &str, value: &Value) -> String {
        let slot = self.temp(temp_prefix);
        self.body
            .push_str(&format!("  %{slot} = alloca %yar.str\n"));
        self.body
            .push_str(&format!("  store %yar.str {}, ptr %{slot}\n", value.repr));
        format!("%{slot}")
    }

    fn emit_host_status_call(
        &mut self,
        signature: &Signature,
        temp_prefix: &str,
        runtime_name: &str,
        call_args: String,
    ) -> Result<Value, CodegenError> {
        let status = self.temp(&format!("{temp_prefix}.status"));
        self.body.push_str(&format!(
            "  %{status} = call i32 @{runtime_name}({call_args})\n"
        ));
        self.emit_host_status_result(
            &signature.full_name,
            &signature.return_type,
            &format!("%{status}"),
            None,
        )
    }

    fn emit_fs_i64_out_call(
        &mut self,
        signature: &Signature,
        temp_prefix: &str,
        runtime_name: &str,
        args: Vec<Value>,
    ) -> Result<Value, CodegenError> {
        let path = self.emit_str_abi_input(&format!("{temp_prefix}.path"), &args[0]);
        let out = self.temp(&format!("{temp_prefix}.out"));
        self.body.push_str(&format!("  %{out} = alloca i64\n"));
        self.body.push_str(&format!("  store i64 0, ptr %{out}\n"));
        let status = self.temp(&format!("{temp_prefix}.status"));
        self.body.push_str(&format!(
            "  %{status} = call i32 @{runtime_name}(ptr {path}, ptr %{out})\n"
        ));
        let value = self.temp(&format!("{temp_prefix}.value"));
        self.body
            .push_str(&format!("  %{value} = load i64, ptr %{out}\n"));
        self.emit_host_status_result(
            &signature.full_name,
            &signature.return_type,
            &format!("%{status}"),
            Some(&format!("%{value}")),
        )
    }

    fn emit_direct_call(
        &mut self,
        signature: &Signature,
        values: Vec<Value>,
    ) -> Result<Value, CodegenError> {
        if signature.params.len() != values.len() {
            return Err(CodegenError::unsupported(format!(
                "call arity for {:?}",
                signature.full_name
            )));
        }
        let mut args = Vec::new();
        for (idx, value) in values.into_iter().enumerate() {
            let expected = signature.params[idx].as_str();
            if value.type_ != expected {
                return Err(CodegenError::unsupported(format!(
                    "argument {} to {:?} has type {}",
                    idx + 1,
                    signature.full_name,
                    value.type_
                )));
            }
            args.push(format!("{} {}", self.llvm_type(expected)?, value.repr));
        }

        let llvm_return = if signature.errorable {
            result_type_name(&signature.return_type)
        } else {
            self.llvm_type(&signature.return_type)?.to_string()
        };
        if !signature.errorable && matches!(signature.return_type.as_str(), "void" | "noreturn") {
            self.body.push_str(&format!(
                "  call void @{}({})\n",
                function_symbol(&signature.full_name),
                args.join(", ")
            ));
            return Ok(Value {
                type_: "void".to_string(),
                repr: String::new(),
            });
        }

        let result = self.temp("call");
        self.body.push_str(&format!(
            "  %{result} = call {llvm_return} @{}({})\n",
            function_symbol(&signature.full_name),
            args.join(", ")
        ));
        Ok(Value {
            type_: if signature.errorable {
                errorable_type(&signature.return_type)
            } else {
                signature.return_type.clone()
            },
            repr: format!("%{result}"),
        })
    }

    fn emit_propagate(&mut self, expr: &PropagateExpr) -> Result<Value, CodegenError> {
        let value = self.emit_expression(&expr.inner)?;
        if value.type_.starts_with('!') {
            let inner_type = value.type_.trim_start_matches('!').to_string();
            let result_type = result_type_name(&inner_type);
            let is_err = self.temp("propagate.is_err");
            let err_label = self.label("propagate.err");
            let ok_label = self.label("propagate.ok");
            self.body.push_str(&format!(
                "  %{is_err} = extractvalue {result_type} {}, 0\n",
                value.repr
            ));
            self.body.push_str(&format!(
                "  br i1 %{is_err}, label %{err_label}, label %{ok_label}\n"
            ));

            self.start_block(&err_label);
            let err_code = self.temp("propagate.err_code");
            self.body.push_str(&format!(
                "  %{err_code} = extractvalue {result_type} {}, 1\n",
                value.repr
            ));
            self.emit_propagated_error(&format!("%{err_code}"))?;

            self.start_block(&ok_label);
            self.terminated = false;
            if inner_type == "void" {
                return Ok(Value {
                    type_: "void".to_string(),
                    repr: String::new(),
                });
            }
            let success = self.temp("propagate.value");
            self.body.push_str(&format!(
                "  %{success} = extractvalue {result_type} {}, 2\n",
                value.repr
            ));
            return Ok(Value {
                type_: inner_type,
                repr: format!("%{success}"),
            });
        }
        if value.type_ == "error" {
            self.emit_propagated_error(&value.repr)?;
            return Ok(Value {
                type_: "void".to_string(),
                repr: String::new(),
            });
        }
        Err(CodegenError::unsupported("propagate non-errorable value"))
    }

    fn emit_handle(&mut self, expr: &HandleExpr) -> Result<Value, CodegenError> {
        let value = self.emit_expression(&expr.inner)?;
        if value.type_.starts_with('!') {
            let inner_type = value.type_.trim_start_matches('!').to_string();
            let result_type = result_type_name(&inner_type);
            let is_err = self.temp("handle.is_err");
            let err_label = self.label("handle.err");
            self.body.push_str(&format!(
                "  %{is_err} = extractvalue {result_type} {}, 0\n",
                value.repr
            ));

            if inner_type != "void" {
                let ok_label = self.label("handle.ok");
                self.body.push_str(&format!(
                    "  br i1 %{is_err}, label %{err_label}, label %{ok_label}\n"
                ));

                self.start_block(&err_label);
                self.terminated = false;
                let err_code = self.temp("handle.err_code");
                self.body.push_str(&format!(
                    "  %{err_code} = extractvalue {result_type} {}, 1\n",
                    value.repr
                ));
                self.emit_error_handler_block(
                    &expr.handler,
                    &expr.err_name,
                    &format!("%{err_code}"),
                )?;
                if !self.terminated {
                    return Err(CodegenError::unsupported(
                        "non-void error handler must terminate",
                    ));
                }

                self.start_block(&ok_label);
                self.terminated = false;
                let success = self.temp("handle.value");
                self.body.push_str(&format!(
                    "  %{success} = extractvalue {result_type} {}, 2\n",
                    value.repr
                ));
                return Ok(Value {
                    type_: inner_type,
                    repr: format!("%{success}"),
                });
            }

            let continue_label = self.label("handle.cont");
            self.body.push_str(&format!(
                "  br i1 %{is_err}, label %{err_label}, label %{continue_label}\n"
            ));

            self.start_block(&err_label);
            self.terminated = false;
            let err_code = self.temp("handle.err_code");
            self.body.push_str(&format!(
                "  %{err_code} = extractvalue {result_type} {}, 1\n",
                value.repr
            ));
            self.emit_error_handler_block(&expr.handler, &expr.err_name, &format!("%{err_code}"))?;
            if !self.terminated {
                self.body
                    .push_str(&format!("  br label %{continue_label}\n"));
            }

            self.start_block(&continue_label);
            self.terminated = false;
            return Ok(Value {
                type_: "void".to_string(),
                repr: String::new(),
            });
        }

        if value.type_ == "error" {
            let is_err = self.temp("handle.is_err");
            let err_label = self.label("handle.err");
            let continue_label = self.label("handle.cont");
            self.body
                .push_str(&format!("  %{is_err} = icmp ne i32 {}, 0\n", value.repr));
            self.body.push_str(&format!(
                "  br i1 %{is_err}, label %{err_label}, label %{continue_label}\n"
            ));

            self.start_block(&err_label);
            self.terminated = false;
            self.emit_error_handler_block(&expr.handler, &expr.err_name, &value.repr)?;
            if !self.terminated {
                self.body
                    .push_str(&format!("  br label %{continue_label}\n"));
            }

            self.start_block(&continue_label);
            self.terminated = false;
            return Ok(Value {
                type_: "void".to_string(),
                repr: String::new(),
            });
        }

        Err(CodegenError::unsupported("handle non-errorable value"))
    }

    fn emit_success_void_result(&mut self) -> Result<Value, CodegenError> {
        let first = self.temp("result");
        let second = self.temp("result");
        let result_type = result_type_name("void");
        self.body.push_str(&format!(
            "  %{first} = insertvalue {result_type} zeroinitializer, i1 false, 0\n"
        ));
        self.body.push_str(&format!(
            "  %{second} = insertvalue {result_type} %{first}, i32 0, 1\n"
        ));
        Ok(Value {
            type_: errorable_type("void"),
            repr: format!("%{second}"),
        })
    }

    fn emit_success_result(&mut self, type_: &str, value: &str) -> Result<Value, CodegenError> {
        let result_type = result_type_name(type_);
        let llvm_type = self.llvm_type(type_)?;
        let first = self.temp("result");
        let second = self.temp("result");
        let third = self.temp("result");
        self.body.push_str(&format!(
            "  %{first} = insertvalue {result_type} zeroinitializer, i1 false, 0\n"
        ));
        self.body.push_str(&format!(
            "  %{second} = insertvalue {result_type} %{first}, i32 0, 1\n"
        ));
        self.body.push_str(&format!(
            "  %{third} = insertvalue {result_type} %{second}, {llvm_type} {value}, 2\n"
        ));
        Ok(Value {
            type_: errorable_type(type_),
            repr: format!("%{third}"),
        })
    }

    fn emit_error_result(&mut self, type_: &str, name: &str) -> Result<Value, CodegenError> {
        let code = self.generator.error_code(name)?;
        self.emit_error_code_result(type_, &code.to_string())
    }

    fn emit_error_code_result(&mut self, type_: &str, code: &str) -> Result<Value, CodegenError> {
        let result_type = result_type_name(type_);
        let first = self.temp("result");
        let second = self.temp("result");
        self.body.push_str(&format!(
            "  %{first} = insertvalue {result_type} zeroinitializer, i1 true, 0\n"
        ));
        self.body.push_str(&format!(
            "  %{second} = insertvalue {result_type} %{first}, i32 {code}, 1\n"
        ));
        if type_ == "void" {
            return Ok(Value {
                type_: errorable_type(type_),
                repr: format!("%{second}"),
            });
        }

        let third = self.temp("result");
        let zero = self.zero_value(type_)?;
        self.body.push_str(&format!(
            "  %{third} = insertvalue {result_type} %{second}, {} {}, 2\n",
            self.llvm_type(type_)?,
            zero.repr
        ));
        Ok(Value {
            type_: errorable_type(type_),
            repr: format!("%{third}"),
        })
    }

    fn emit_closed_status_result(
        &mut self,
        type_: &str,
        status: &str,
        success_value: Option<&str>,
    ) -> Result<Value, CodegenError> {
        let is_ok = self.temp("chan.ok");
        let ok_label = self.label("chan.ok");
        let err_label = self.label("chan.err");
        let end_label = self.label("chan.end");
        self.body
            .push_str(&format!("  %{is_ok} = icmp eq i32 {status}, 0\n"));
        self.body.push_str(&format!(
            "  br i1 %{is_ok}, label %{ok_label}, label %{err_label}\n"
        ));
        self.terminated = true;

        self.start_block(&ok_label);
        self.terminated = false;
        let ok_result = if type_ == "void" {
            self.emit_success_void_result()?
        } else {
            let value = success_value.ok_or_else(|| {
                CodegenError::unsupported(format!("missing channel success value for {type_}"))
            })?;
            self.emit_success_result(type_, value)?
        };
        self.body.push_str(&format!("  br label %{end_label}\n"));

        self.start_block(&err_label);
        self.terminated = false;
        let closed_code = self.generator.error_code("error.Closed")?;
        let err_result = self.emit_error_code_result(type_, &closed_code.to_string())?;
        self.body.push_str(&format!("  br label %{end_label}\n"));

        self.start_block(&end_label);
        self.terminated = false;
        let result_type = result_type_name(type_);
        let result = self.temp("chan.result");
        self.body.push_str(&format!(
            "  %{result} = phi {result_type} [{}, %{ok_label}], [{}, %{err_label}]\n",
            ok_result.repr, err_result.repr
        ));
        Ok(Value {
            type_: errorable_type(type_),
            repr: format!("%{result}"),
        })
    }

    fn emit_host_status_result(
        &mut self,
        full_name: &str,
        type_: &str,
        status: &str,
        success_value: Option<&str>,
    ) -> Result<Value, CodegenError> {
        let is_ok = self.temp("host.ok");
        let ok_label = self.label("host.ok");
        let err_label = self.label("host.err");
        let end_label = self.label("host.end");
        self.body
            .push_str(&format!("  %{is_ok} = icmp eq i32 {status}, 0\n"));
        self.body.push_str(&format!(
            "  br i1 %{is_ok}, label %{ok_label}, label %{err_label}\n"
        ));
        self.terminated = true;

        self.start_block(&ok_label);
        self.terminated = false;
        let ok_result = if type_ == "void" {
            self.emit_success_void_result()?
        } else {
            let value = success_value.ok_or_else(|| {
                CodegenError::unsupported(format!("missing host success value for {type_}"))
            })?;
            self.emit_success_result(type_, value)?
        };
        self.body.push_str(&format!("  br label %{end_label}\n"));

        self.start_block(&err_label);
        self.terminated = false;
        let err_code = self.emit_host_error_code(full_name, status)?;
        let err_result = self.emit_error_code_result(type_, &err_code)?;
        self.body.push_str(&format!("  br label %{end_label}\n"));

        self.start_block(&end_label);
        self.terminated = false;
        let result_type = result_type_name(type_);
        let result = self.temp("host.result");
        self.body.push_str(&format!(
            "  %{result} = phi {result_type} [{}, %{ok_label}], [{}, %{err_label}]\n",
            ok_result.repr, err_result.repr
        ));
        Ok(Value {
            type_: errorable_type(type_),
            repr: format!("%{result}"),
        })
    }

    fn emit_host_error_code(
        &mut self,
        full_name: &str,
        status: &str,
    ) -> Result<String, CodegenError> {
        let owner = host_error_owner(full_name).ok_or_else(|| {
            CodegenError::unsupported(format!("host error mapping for {full_name:?}"))
        })?;
        let mut code = self
            .generator
            .error_code(&format!("{owner}.IO"))?
            .to_string();
        let mappings = if is_fs_host_intrinsic(full_name) {
            &[
                (7, "Closed"),
                (6, "InvalidArgument"),
                (4, "InvalidPath"),
                (3, "AlreadyExists"),
                (2, "PermissionDenied"),
                (1, "NotFound"),
            ][..]
        } else if is_process_host_intrinsic(full_name) {
            &[
                (7, "Cancelled"),
                (6, "LimitExceeded"),
                (5, "Timeout"),
                (3, "InvalidArgument"),
                (2, "PermissionDenied"),
                (1, "NotFound"),
            ][..]
        } else if is_env_host_intrinsic(full_name) {
            &[
                (3, "InvalidArgument"),
                (2, "PermissionDenied"),
                (1, "NotFound"),
            ][..]
        } else if is_net_host_intrinsic(full_name) {
            &[
                (9, "Closed"),
                (7, "InvalidArgument"),
                (6, "PermissionDenied"),
                (5, "NotFound"),
                (4, "ConnectionReset"),
                (3, "AddrInUse"),
                (2, "Timeout"),
                (1, "ConnectionRefused"),
            ][..]
        } else {
            return Err(CodegenError::unsupported(format!(
                "host error mapping for {full_name:?}"
            )));
        };

        for (status_code, name) in mappings {
            let identity = host_error_identity(owner, name);
            let matched = self.temp("host.err.match");
            let next = self.temp("host.err.code");
            self.body.push_str(&format!(
                "  %{matched} = icmp eq i32 {status}, {status_code}\n"
            ));
            self.body.push_str(&format!(
                "  %{next} = select i1 %{matched}, i32 {}, i32 {code}\n",
                self.generator.error_code(&identity)?
            ));
            code = format!("%{next}");
        }
        Ok(code)
    }

    fn emit_propagated_error(&mut self, code: &str) -> Result<(), CodegenError> {
        if !self.signature.errorable {
            if self.signature.return_type == "error" {
                self.body.push_str(&format!("  ret i32 {code}\n"));
                self.terminated = true;
                return Ok(());
            }
            return Err(CodegenError::unsupported(
                "propagation requires an error-capable return",
            ));
        }

        let return_type = self.signature.return_type.clone();
        let result = self.emit_error_code_result(&return_type, code)?;
        self.body.push_str(&format!(
            "  ret {} {}\n",
            result_type_name(&return_type),
            result.repr
        ));
        self.terminated = true;
        Ok(())
    }

    fn emit_expression_as(
        &mut self,
        expression: &Expression,
        expected_type: &str,
    ) -> Result<Value, CodegenError> {
        if matches!(expression, Expression::Nil(_)) && parse_pointer_type(expected_type).is_some() {
            return Ok(Value {
                type_: expected_type.to_string(),
                repr: "null".to_string(),
            });
        }
        if matches!(expected_type, "i32" | "i64") && is_untyped_integer_expression(expression) {
            return self.emit_untyped_integer_expression_as(expression, expected_type);
        }
        let value = self.emit_expression(expression)?;
        if self.generator.info.interfaces.contains_key(expected_type) {
            return self.coerce_to_interface(value, expected_type);
        }
        Ok(value)
    }

    fn emit_untyped_integer_expression_as(
        &mut self,
        expression: &Expression,
        expected_type: &str,
    ) -> Result<Value, CodegenError> {
        if let Some(value) = const_integer_expression(expression) {
            let repr = match expected_type {
                "i32" => i32::try_from(value)
                    .map_err(|_| CodegenError::unsupported("i32 constant expression"))?
                    .to_string(),
                "i64" => value.to_string(),
                _ => {
                    return Err(CodegenError::unsupported("integer expression target type"));
                }
            };
            return Ok(Value {
                type_: expected_type.to_string(),
                repr,
            });
        }

        match expression {
            Expression::Group(expr) => {
                self.emit_untyped_integer_expression_as(&expr.inner, expected_type)
            }
            Expression::Unary(expr) if expr.operator == Kind::Minus => {
                let value = self.emit_untyped_integer_expression_as(&expr.inner, expected_type)?;
                self.emit_arithmetic_value(
                    Kind::Minus,
                    Value {
                        type_: expected_type.to_string(),
                        repr: "0".to_string(),
                    },
                    value,
                )
            }
            Expression::Binary(expr)
                if matches!(
                    expr.operator,
                    Kind::Plus | Kind::Minus | Kind::Star | Kind::Slash | Kind::Percent
                ) =>
            {
                let left = self.emit_untyped_integer_expression_as(&expr.left, expected_type)?;
                let right = self.emit_untyped_integer_expression_as(&expr.right, expected_type)?;
                self.emit_arithmetic_value(expr.operator, left, right)
            }
            _ => Err(CodegenError::unsupported("untyped integer expression")),
        }
    }

    fn coerce_to_interface(
        &mut self,
        value: Value,
        interface_type: &str,
    ) -> Result<Value, CodegenError> {
        if value.type_ == interface_type {
            return Ok(value);
        }
        let table = self
            .generator
            .ensure_interface_impl(interface_type, &value.type_)?;
        let data = if parse_pointer_type(&value.type_).is_some() {
            value.repr
        } else {
            let slot = self.emit_alloc_type(&value.type_, false)?;
            self.body.push_str(&format!(
                "  store {} {}, ptr {slot}\n",
                self.llvm_type(&value.type_)?,
                value.repr
            ));
            slot
        };
        Ok(Value {
            type_: interface_type.to_string(),
            repr: self.emit_interface_value(&data, &table),
        })
    }

    fn emit_interface_value(&mut self, data: &str, table: &str) -> String {
        let first = self.temp("iface");
        let second = self.temp("iface");
        self.body.push_str(&format!(
            "  %{first} = insertvalue %yar.iface zeroinitializer, ptr {data}, 0\n"
        ));
        self.body.push_str(&format!(
            "  %{second} = insertvalue %yar.iface %{first}, ptr {table}, 1\n"
        ));
        format!("%{second}")
    }

    fn emit_error_handler_block(
        &mut self,
        block: &BlockStmt,
        err_name: &str,
        code: &str,
    ) -> Result<(), CodegenError> {
        let previous = self.locals.get(err_name).cloned();
        let slot = self.allocate_local_slot(err_name, "error")?;
        self.body
            .push_str(&format!("  store i32 {code}, ptr {slot}\n"));
        self.locals.insert(
            err_name.to_string(),
            Local {
                type_: "error".to_string(),
                ptr: slot,
            },
        );

        for statement in &block.stmts {
            self.emit_statement(statement)?;
            if self.terminated {
                break;
            }
        }

        match previous {
            Some(local) => {
                self.locals.insert(err_name.to_string(), local);
            }
            None => {
                self.locals.remove(err_name);
            }
        }
        Ok(())
    }

    fn expression_type_hint(&self, expression: &Expression) -> Option<String> {
        match expression {
            Expression::Ident(expr) => self.locals.get(&expr.name).map(|local| local.type_.clone()),
            Expression::Group(expr) => self.expression_type_hint(&expr.inner),
            Expression::StructLiteral(expr) => Some(expr.type_ref.to_string()),
            Expression::ArrayLiteral(expr) => Some(expr.type_ref.to_string()),
            Expression::SliceLiteral(expr) => Some(expr.type_ref.to_string()),
            Expression::MapLiteral(expr) => Some(expr.type_ref.to_string()),
            Expression::Selector(expr) => {
                let inner = self.expression_type_hint(&expr.inner)?;
                let base = parse_pointer_type(&inner).unwrap_or(inner);
                self.generator
                    .info
                    .structs
                    .get(&base)?
                    .fields
                    .iter()
                    .find(|field| field.name == expr.name)
                    .map(|field| field.type_.clone())
            }
            Expression::FunctionLiteral(expr) => {
                let key = function_literal_key(expr);
                self.generator.info.function_literals.get(&key).map(|info| {
                    format_function_type(
                        &info.signature.params,
                        &info.signature.return_type,
                        info.signature.errorable,
                    )
                })
            }
            _ => None,
        }
    }

    fn emit_stack_slot(
        &mut self,
        label: &str,
        type_: &str,
        value: &str,
    ) -> Result<String, CodegenError> {
        let slot = self.temp(label);
        let llvm_type = self.llvm_type(type_)?;
        self.body
            .push_str(&format!("  %{slot} = alloca {llvm_type}\n"));
        self.body
            .push_str(&format!("  store {llvm_type} {value}, ptr %{slot}\n"));
        Ok(format!("%{slot}"))
    }

    fn emit_struct_field_extract(
        &mut self,
        label: &str,
        value: &Value,
        field_name: &str,
    ) -> Result<Value, CodegenError> {
        let (field_index, field_type) = self.generator.struct_field(&value.type_, field_name)?;
        let result = self.temp(label);
        self.body.push_str(&format!(
            "  %{result} = extractvalue {} {}, {field_index}\n",
            self.llvm_type(&value.type_)?,
            value.repr
        ));
        Ok(Value {
            type_: field_type,
            repr: format!("%{result}"),
        })
    }

    fn emit_map_set(
        &mut self,
        map: &str,
        key_type: &str,
        key: &Value,
        value_type: &str,
        value: &Value,
    ) -> Result<(), CodegenError> {
        if key.type_ != key_type {
            return Err(CodegenError::unsupported(format!(
                "map key has type {}",
                key.type_
            )));
        }
        if value.type_ != value_type {
            return Err(CodegenError::unsupported(format!(
                "map value has type {}",
                value.type_
            )));
        }

        let key_slot = self.emit_stack_slot("map.key.slot", key_type, &key.repr)?;
        let value_slot = self.emit_stack_slot("map.val.slot", value_type, &value.repr)?;
        self.body.push_str(&format!(
            "  call void @yar_map_set(ptr {map}, ptr {key_slot}, ptr {value_slot})\n"
        ));
        Ok(())
    }

    fn extract_slice_field(
        &mut self,
        slice: &str,
        idx: usize,
        label: &str,
    ) -> Result<Value, CodegenError> {
        let result = self.temp(label);
        match idx {
            0 => {
                self.body.push_str(&format!(
                    "  %{result} = extractvalue %yar.slice {slice}, 0\n"
                ));
                Ok(Value {
                    type_: "*data".to_string(),
                    repr: format!("%{result}"),
                })
            }
            1 | 2 => {
                self.body.push_str(&format!(
                    "  %{result} = extractvalue %yar.slice {slice}, {idx}\n"
                ));
                Ok(Value {
                    type_: "i32".to_string(),
                    repr: format!("%{result}"),
                })
            }
            _ => Err(CodegenError::unsupported("slice field index")),
        }
    }

    fn emit_slice_value(&mut self, data: &str, len: &str, cap: &str) -> String {
        let first = self.temp("slice");
        let second = self.temp("slice");
        let third = self.temp("slice");
        self.body.push_str(&format!(
            "  %{first} = insertvalue %yar.slice zeroinitializer, ptr {data}, 0\n"
        ));
        self.body.push_str(&format!(
            "  %{second} = insertvalue %yar.slice %{first}, i32 {len}, 1\n"
        ));
        self.body.push_str(&format!(
            "  %{third} = insertvalue %yar.slice %{second}, i32 {cap}, 2\n"
        ));
        format!("%{third}")
    }

    fn enum_payload_ptr(
        &mut self,
        enum_slot: &str,
        enum_type: &str,
    ) -> Result<String, CodegenError> {
        let payload_ptr = self.temp("enum.payload.ptr");
        self.body.push_str(&format!(
            "  %{payload_ptr} = getelementptr inbounds {}, ptr %{enum_slot}, i32 0, i32 1\n",
            self.llvm_type(enum_type)?
        ));
        Ok(format!("%{payload_ptr}"))
    }

    fn emit_alloc_bytes(&mut self, size: &str, zeroed: bool) -> String {
        let helper = if zeroed {
            "@yar_alloc_zeroed"
        } else {
            "@yar_alloc"
        };
        let result = self.temp("alloc");
        self.body
            .push_str(&format!("  %{result} = call ptr {helper}(i64 {size})\n"));
        format!("%{result}")
    }

    fn emit_alloc_type(&mut self, type_: &str, zeroed: bool) -> Result<String, CodegenError> {
        let size = self.emit_type_size(type_)?;
        Ok(self.emit_alloc_bytes(&size, zeroed))
    }

    fn emit_array_alloc_size(
        &mut self,
        element_type: &str,
        len: usize,
    ) -> Result<String, CodegenError> {
        self.emit_scaled_size(element_type, &len.to_string())
    }

    fn emit_scaled_size(
        &mut self,
        element_type: &str,
        len64: &str,
    ) -> Result<String, CodegenError> {
        let size = self.emit_type_size(element_type)?;
        if size == "1" {
            return Ok(len64.to_string());
        }
        let result = self.temp("alloc.size");
        self.body
            .push_str(&format!("  %{result} = mul i64 {len64}, {size}\n"));
        Ok(format!("%{result}"))
    }

    fn emit_type_size(&mut self, type_: &str) -> Result<String, CodegenError> {
        let llvm_type = self.llvm_type(type_)?;
        let size_ptr = self.temp("size.ptr");
        let size = self.temp("size");
        self.body.push_str(&format!(
            "  %{size_ptr} = getelementptr {llvm_type}, ptr null, i32 1\n"
        ));
        self.body
            .push_str(&format!("  %{size} = ptrtoint ptr %{size_ptr} to i64\n"));
        Ok(format!("%{size}"))
    }

    fn emit_memcpy(&mut self, dst: &str, src: &str, size: &str) {
        self.body.push_str(&format!(
            "  call void @llvm.memcpy.p0.p0.i64(ptr {dst}, ptr {src}, i64 {size}, i1 false)\n"
        ));
    }

    fn emit_closure_value(&mut self, code: &str, env: &str) -> String {
        let first = self.temp("closure");
        let second = self.temp("closure");
        self.body.push_str(&format!(
            "  %{first} = insertvalue %yar.closure zeroinitializer, ptr {code}, 0\n"
        ));
        self.body.push_str(&format!(
            "  %{second} = insertvalue %yar.closure %{first}, ptr {env}, 1\n"
        ));
        format!("%{second}")
    }

    fn closure_env_type_literal(&self, captures: &[CaptureInfo]) -> Result<String, CodegenError> {
        if captures.is_empty() {
            return Ok("{ }".to_string());
        }
        let fields = captures
            .iter()
            .map(|capture| self.llvm_type(&capture.type_))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(format!("{{ {} }}", fields.join(", ")))
    }

    fn closure_env_size(&self, captures: &[CaptureInfo]) -> Result<i32, CodegenError> {
        let mut size = 0_i32;
        let mut align = 1_i32;
        for capture in captures {
            let field_align = self.type_align(&capture.type_)?;
            size = align_to(size, field_align)?;
            size = size
                .checked_add(self.type_size(&capture.type_)?)
                .ok_or_else(|| CodegenError::unsupported("closure env size overflow"))?;
            align = align.max(field_align);
        }
        align_to(size, align)
    }

    fn type_size(&self, type_: &str) -> Result<i32, CodegenError> {
        self.generator.type_size(type_)
    }

    fn type_align(&self, type_: &str) -> Result<i32, CodegenError> {
        self.generator.type_align(type_)
    }

    fn llvm_type(&self, type_: &str) -> Result<String, CodegenError> {
        self.generator.llvm_type(type_)
    }

    fn zero_value(&self, type_: &str) -> Result<Value, CodegenError> {
        match type_ {
            "bool" => Ok(Value {
                type_: type_.to_string(),
                repr: "false".to_string(),
            }),
            "i32" | "i64" => Ok(Value {
                type_: type_.to_string(),
                repr: "0".to_string(),
            }),
            "str" => Ok(Value {
                type_: type_.to_string(),
                repr: "zeroinitializer".to_string(),
            }),
            other if self.generator.info.structs.contains_key(other) => Ok(Value {
                type_: type_.to_string(),
                repr: "zeroinitializer".to_string(),
            }),
            other if self.generator.info.enums.contains_key(other) => Ok(Value {
                type_: type_.to_string(),
                repr: "zeroinitializer".to_string(),
            }),
            other if self.generator.info.interfaces.contains_key(other) => Ok(Value {
                type_: type_.to_string(),
                repr: "zeroinitializer".to_string(),
            }),
            other if parse_array_type(other).is_some() => Ok(Value {
                type_: type_.to_string(),
                repr: "zeroinitializer".to_string(),
            }),
            other if parse_slice_type(other).is_some() => Ok(Value {
                type_: type_.to_string(),
                repr: "zeroinitializer".to_string(),
            }),
            other if parse_function_type(other).is_some() => Ok(Value {
                type_: type_.to_string(),
                repr: "zeroinitializer".to_string(),
            }),
            other if parse_pointer_type(other).is_some() => Ok(Value {
                type_: type_.to_string(),
                repr: "null".to_string(),
            }),
            other if parse_map_type(other).is_some() => Ok(Value {
                type_: type_.to_string(),
                repr: "null".to_string(),
            }),
            other if parse_chan_type(other).is_some() => Ok(Value {
                type_: type_.to_string(),
                repr: "null".to_string(),
            }),
            other => Err(CodegenError::unsupported(format!(
                "zero value for {other:?}"
            ))),
        }
    }

    fn temp(&mut self, label: &str) -> String {
        let id = self.next_id;
        self.next_id += 1;
        format!("{label}.{id}")
    }

    fn int_to_i64(&mut self, value: &Value) -> Result<String, CodegenError> {
        match value.type_.as_str() {
            "i64" => Ok(value.repr.clone()),
            "i32" => {
                let result = self.temp("widen");
                self.body
                    .push_str(&format!("  %{result} = sext i32 {} to i64\n", value.repr));
                Ok(format!("%{result}"))
            }
            other => Err(CodegenError::unsupported(format!(
                "integer conversion from {other}"
            ))),
        }
    }

    fn label(&mut self, label: &str) -> String {
        self.temp(label)
    }

    fn start_block(&mut self, label: &str) {
        self.body.push_str(&format!("{label}:\n"));
        self.current_block = label.to_string();
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Local {
    type_: String,
    ptr: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Address {
    type_: String,
    ptr: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LoopLabels {
    break_label: String,
    continue_label: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TaskgroupContext {
    handle: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FunctionParam {
    name: String,
    type_: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Value {
    type_: String,
    repr: String,
}

fn llvm_type(type_: &str) -> Result<String, CodegenError> {
    if let Some(inner) = type_.strip_prefix('!') {
        return Ok(result_type_name(inner));
    }
    match type_ {
        "void" => Ok("void".to_string()),
        "noreturn" => Ok("void".to_string()),
        "bool" => Ok("i1".to_string()),
        "i32" => Ok("i32".to_string()),
        "i64" => Ok("i64".to_string()),
        "str" => Ok("%yar.str".to_string()),
        "error" => Ok("i32".to_string()),
        other => {
            if parse_pointer_type(other).is_some() {
                return Ok("ptr".to_string());
            }
            if parse_function_type(other).is_some() {
                return Ok("%yar.closure".to_string());
            }
            if parse_map_type(other).is_some() {
                return Ok("ptr".to_string());
            }
            if parse_chan_type(other).is_some() {
                return Ok("ptr".to_string());
            }
            if parse_slice_type(other).is_some() {
                return Ok("%yar.slice".to_string());
            }
            if let Some((len, elem)) = parse_array_type(other) {
                return Ok(format!("[{len} x {}]", llvm_type(&elem)?));
            }
            Ok(struct_type_name(other))
        }
    }
}

fn parse_array_type(type_: &str) -> Option<(usize, String)> {
    let rest = type_.strip_prefix('[')?;
    let end = rest.find(']')?;
    let len = rest[..end].parse::<usize>().ok()?;
    let elem = rest[end + 1..].to_string();
    if elem.is_empty() {
        return None;
    }
    Some((len, elem))
}

fn parse_slice_type(type_: &str) -> Option<String> {
    type_.strip_prefix("[]").map(ToString::to_string)
}

fn parse_pointer_type(type_: &str) -> Option<String> {
    type_.strip_prefix('*').map(ToString::to_string)
}

fn parse_map_type(type_: &str) -> Option<(String, String)> {
    let rest = type_.strip_prefix("map[")?;
    let mut depth = 0usize;
    for (idx, ch) in rest.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' if depth == 0 => {
                let key = rest[..idx].to_string();
                let value = rest[idx + 1..].to_string();
                if key.is_empty() || value.is_empty() {
                    return None;
                }
                return Some((key, value));
            }
            ']' => depth = depth.checked_sub(1)?,
            _ => {}
        }
    }
    None
}

fn parse_chan_type(type_: &str) -> Option<String> {
    let inner = type_.strip_prefix("chan[")?.strip_suffix(']')?;
    if inner.is_empty() {
        return None;
    }
    Some(inner.to_string())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FunctionType {
    params: Vec<String>,
    return_type: String,
    errorable: bool,
}

fn parse_function_type(type_: &str) -> Option<FunctionType> {
    let rest = type_.strip_prefix("fn(")?;
    let close = find_matching_paren(rest)?;
    let params_text = &rest[..close];
    let mut return_text = rest[close + 1..].trim_start();
    if return_text.is_empty() {
        return None;
    }
    let errorable = return_text.starts_with('!');
    if errorable {
        return_text = &return_text[1..];
    }
    if return_text.is_empty() {
        return None;
    }
    Some(FunctionType {
        params: split_top_level_types(params_text)?,
        return_type: return_text.to_string(),
        errorable,
    })
}

fn format_function_type(params: &[String], return_type: &str, errorable: bool) -> String {
    let bang = if errorable { "!" } else { "" };
    format!("fn({}) {bang}{return_type}", params.join(", "))
}

fn find_matching_paren(text: &str) -> Option<usize> {
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    for (idx, ch) in text.char_indices() {
        match ch {
            '[' => bracket_depth += 1,
            ']' if bracket_depth > 0 => bracket_depth -= 1,
            '(' => paren_depth += 1,
            ')' if bracket_depth == 0 && paren_depth == 0 => return Some(idx),
            ')' if paren_depth > 0 => paren_depth -= 1,
            _ => {}
        }
    }
    None
}

fn split_top_level_types(text: &str) -> Option<Vec<String>> {
    if text.trim().is_empty() {
        return Some(Vec::new());
    }

    let mut out = Vec::new();
    let mut start = 0usize;
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    for (idx, ch) in text.char_indices() {
        match ch {
            '[' => bracket_depth += 1,
            ']' if bracket_depth > 0 => bracket_depth -= 1,
            '(' => paren_depth += 1,
            ')' if paren_depth > 0 => paren_depth -= 1,
            ',' if bracket_depth == 0 && paren_depth == 0 => {
                let part = text[start..idx].trim();
                if part.is_empty() {
                    return None;
                }
                out.push(part.to_string());
                start = idx + 1;
            }
            _ => {}
        }
    }
    let part = text[start..].trim();
    if part.is_empty() {
        return None;
    }
    out.push(part.to_string());
    Some(out)
}

fn map_key_kind(type_: &str) -> Result<i32, CodegenError> {
    match type_ {
        "bool" => Ok(0),
        "i32" => Ok(1),
        "i64" => Ok(2),
        "str" => Ok(3),
        other => Err(CodegenError::unsupported(format!("map key type {other:?}"))),
    }
}

fn align_to(size: i32, align: i32) -> Result<i32, CodegenError> {
    if align <= 0 {
        return Err(CodegenError::unsupported("invalid type alignment"));
    }
    let adjusted = size
        .checked_add(align - 1)
        .ok_or_else(|| CodegenError::unsupported("type size overflow"))?;
    Ok(adjusted / align * align)
}

fn collect_function_literals(program: &Program) -> Vec<&FunctionLiteralExpr> {
    let mut literals = Vec::new();
    for function in &program.functions {
        collect_function_literals_from_block(&function.body, &mut literals);
    }
    literals
}

fn collect_function_literals_from_block<'a>(
    block: &'a BlockStmt,
    literals: &mut Vec<&'a FunctionLiteralExpr>,
) {
    for statement in &block.stmts {
        collect_function_literals_from_statement(statement, literals);
    }
}

fn collect_function_literals_from_statement<'a>(
    statement: &'a Statement,
    literals: &mut Vec<&'a FunctionLiteralExpr>,
) {
    match statement {
        Statement::Block(block) => collect_function_literals_from_block(block, literals),
        Statement::Let(stmt) => collect_function_literals_from_expression(&stmt.value, literals),
        Statement::Var(stmt) => {
            if let Some(value) = &stmt.value {
                collect_function_literals_from_expression(value, literals);
            }
        }
        Statement::Assign(stmt) => {
            collect_function_literals_from_expression(&stmt.target, literals);
            collect_function_literals_from_expression(&stmt.value, literals);
        }
        Statement::CompoundAssign(stmt) => {
            collect_function_literals_from_expression(&stmt.target, literals);
            collect_function_literals_from_expression(&stmt.value, literals);
        }
        Statement::If(stmt) => {
            collect_function_literals_from_expression(&stmt.cond, literals);
            collect_function_literals_from_block(&stmt.then_block, literals);
            if let Some(else_stmt) = &stmt.else_stmt {
                collect_function_literals_from_statement(else_stmt, literals);
            }
        }
        Statement::For(stmt) => {
            if let Some(init) = &stmt.init {
                collect_function_literals_from_statement(init, literals);
            }
            if let Some(cond) = &stmt.cond {
                collect_function_literals_from_expression(cond, literals);
            }
            if let Some(post) = &stmt.post {
                collect_function_literals_from_statement(post, literals);
            }
            collect_function_literals_from_block(&stmt.body, literals);
        }
        Statement::Return(stmt) => {
            if let Some(value) = &stmt.value {
                collect_function_literals_from_expression(value, literals);
            }
        }
        Statement::Match(stmt) => {
            collect_function_literals_from_expression(&stmt.value, literals);
            for arm in &stmt.arms {
                collect_function_literals_from_block(&arm.body, literals);
            }
            if let Some(else_body) = &stmt.else_body {
                collect_function_literals_from_block(else_body, literals);
            }
        }
        Statement::Expr(stmt) => collect_function_literals_from_expression(&stmt.expr, literals),
        Statement::Spawn(stmt) => collect_function_literals_from_expression(&stmt.call, literals),
        Statement::Break(_) | Statement::Continue(_) => {}
    }
}

fn collect_function_literals_from_expression<'a>(
    expression: &'a Expression,
    literals: &mut Vec<&'a FunctionLiteralExpr>,
) {
    match expression {
        Expression::TypeApplication(expr) => {
            collect_function_literals_from_expression(&expr.inner, literals);
        }
        Expression::Call(expr) => {
            collect_function_literals_from_expression(&expr.callee, literals);
            for arg in &expr.args {
                collect_function_literals_from_expression(arg, literals);
            }
        }
        Expression::FunctionLiteral(expr) => {
            literals.push(expr);
            collect_function_literals_from_block(&expr.body, literals);
        }
        Expression::Taskgroup(expr) => collect_function_literals_from_block(&expr.body, literals),
        Expression::Unary(expr) => collect_function_literals_from_expression(&expr.inner, literals),
        Expression::Binary(expr) => {
            collect_function_literals_from_expression(&expr.left, literals);
            collect_function_literals_from_expression(&expr.right, literals);
        }
        Expression::Group(expr) => collect_function_literals_from_expression(&expr.inner, literals),
        Expression::Selector(expr) => {
            collect_function_literals_from_expression(&expr.inner, literals);
        }
        Expression::Index(expr) => {
            collect_function_literals_from_expression(&expr.inner, literals);
            collect_function_literals_from_expression(&expr.index, literals);
        }
        Expression::Slice(expr) => {
            collect_function_literals_from_expression(&expr.inner, literals);
            if let Some(start) = &expr.start {
                collect_function_literals_from_expression(start, literals);
            }
            if let Some(end) = &expr.end {
                collect_function_literals_from_expression(end, literals);
            }
        }
        Expression::StructLiteral(expr) => {
            for field in &expr.fields {
                collect_function_literals_from_expression(&field.value, literals);
            }
        }
        Expression::ArrayLiteral(expr) => {
            for element in &expr.elements {
                collect_function_literals_from_expression(element, literals);
            }
        }
        Expression::SliceLiteral(expr) => {
            for element in &expr.elements {
                collect_function_literals_from_expression(element, literals);
            }
        }
        Expression::MapLiteral(expr) => {
            for pair in &expr.pairs {
                collect_function_literals_from_expression(&pair.key, literals);
                collect_function_literals_from_expression(&pair.value, literals);
            }
        }
        Expression::Propagate(expr) => {
            collect_function_literals_from_expression(&expr.inner, literals);
        }
        Expression::Handle(expr) => {
            collect_function_literals_from_expression(&expr.inner, literals);
            collect_function_literals_from_block(&expr.handler, literals);
        }
        Expression::Ident(_)
        | Expression::Int(_)
        | Expression::Char(_)
        | Expression::String(_)
        | Expression::Bool(_)
        | Expression::Nil(_)
        | Expression::Error(_)
        | Expression::Missing(_) => {}
    }
}

fn collect_map_result_types(program: &Program, result_types: &mut Vec<String>) {
    for decl in &program.structs {
        for field in &decl.fields {
            collect_map_result_types_from_type_ref(&field.type_ref, result_types);
        }
    }
    for decl in &program.enums {
        for case in &decl.cases {
            for field in &case.fields {
                collect_map_result_types_from_type_ref(&field.type_ref, result_types);
            }
        }
    }
    for function in &program.functions {
        if let Some(receiver) = &function.receiver {
            collect_map_result_types_from_type_ref(&receiver.type_ref, result_types);
        }
        for param in &function.params {
            collect_map_result_types_from_type_ref(&param.type_ref, result_types);
        }
        collect_map_result_types_from_type_ref(&function.return_type, result_types);
        collect_map_result_types_from_block(&function.body, result_types);
    }
}

fn collect_map_result_types_from_type_ref(type_ref: &TypeRef, result_types: &mut Vec<String>) {
    if let Some((_, value)) = parse_map_type(&type_ref.to_string()) {
        result_types.push(value);
    }
    if let Some(value) = parse_chan_type(&type_ref.to_string()) {
        result_types.push("void".to_string());
        result_types.push(value);
    }
    for arg in &type_ref.type_args {
        collect_map_result_types_from_type_ref(arg, result_types);
    }
    if let Some(elem) = type_ref.elem.as_deref() {
        collect_map_result_types_from_type_ref(elem, result_types);
    }
    if let Some(key) = type_ref.key.as_deref() {
        collect_map_result_types_from_type_ref(key, result_types);
    }
    if let Some(value) = type_ref.value.as_deref() {
        collect_map_result_types_from_type_ref(value, result_types);
    }
    for param in &type_ref.params {
        collect_map_result_types_from_type_ref(param, result_types);
    }
    if let Some(return_type) = type_ref.return_type.as_deref() {
        collect_map_result_types_from_type_ref(return_type, result_types);
    }
}

fn collect_map_result_types_from_block(block: &BlockStmt, result_types: &mut Vec<String>) {
    for statement in &block.stmts {
        collect_map_result_types_from_statement(statement, result_types);
    }
}

fn collect_map_result_types_from_statement(statement: &Statement, result_types: &mut Vec<String>) {
    match statement {
        Statement::Block(block) => collect_map_result_types_from_block(block, result_types),
        Statement::Let(stmt) => collect_map_result_types_from_expression(&stmt.value, result_types),
        Statement::Var(stmt) => {
            collect_map_result_types_from_type_ref(&stmt.type_ref, result_types);
            if let Some(value) = &stmt.value {
                collect_map_result_types_from_expression(value, result_types);
            }
        }
        Statement::Assign(stmt) => {
            collect_map_result_types_from_expression(&stmt.target, result_types);
            collect_map_result_types_from_expression(&stmt.value, result_types);
        }
        Statement::CompoundAssign(stmt) => {
            collect_map_result_types_from_expression(&stmt.target, result_types);
            collect_map_result_types_from_expression(&stmt.value, result_types);
        }
        Statement::If(stmt) => {
            collect_map_result_types_from_expression(&stmt.cond, result_types);
            collect_map_result_types_from_block(&stmt.then_block, result_types);
            if let Some(else_stmt) = &stmt.else_stmt {
                collect_map_result_types_from_statement(else_stmt, result_types);
            }
        }
        Statement::For(stmt) => {
            if let Some(init) = &stmt.init {
                collect_map_result_types_from_statement(init, result_types);
            }
            if let Some(cond) = &stmt.cond {
                collect_map_result_types_from_expression(cond, result_types);
            }
            if let Some(post) = &stmt.post {
                collect_map_result_types_from_statement(post, result_types);
            }
            collect_map_result_types_from_block(&stmt.body, result_types);
        }
        Statement::Return(stmt) => {
            if let Some(value) = &stmt.value {
                collect_map_result_types_from_expression(value, result_types);
            }
        }
        Statement::Match(stmt) => {
            collect_map_result_types_from_expression(&stmt.value, result_types);
            for arm in &stmt.arms {
                collect_map_result_types_from_type_ref(&arm.enum_type, result_types);
                collect_map_result_types_from_block(&arm.body, result_types);
            }
            if let Some(else_body) = &stmt.else_body {
                collect_map_result_types_from_block(else_body, result_types);
            }
        }
        Statement::Expr(stmt) => collect_map_result_types_from_expression(&stmt.expr, result_types),
        Statement::Spawn(stmt) => {
            collect_map_result_types_from_expression(&stmt.call, result_types)
        }
        Statement::Break(_) | Statement::Continue(_) => {}
    }
}

fn collect_map_result_types_from_expression(
    expression: &Expression,
    result_types: &mut Vec<String>,
) {
    match expression {
        Expression::TypeApplication(expr) => {
            collect_map_result_types_from_expression(&expr.inner, result_types);
            if matches!(&expr.inner, Expression::Ident(ident) if ident.name == "chan_new")
                && let [element_type] = expr.type_args.as_slice()
            {
                result_types.push("void".to_string());
                result_types.push(element_type.to_string());
            }
            for type_arg in &expr.type_args {
                collect_map_result_types_from_type_ref(type_arg, result_types);
            }
        }
        Expression::Call(expr) => {
            collect_map_result_types_from_expression(&expr.callee, result_types);
            for arg in &expr.args {
                collect_map_result_types_from_expression(arg, result_types);
            }
        }
        Expression::FunctionLiteral(expr) => {
            for param in &expr.params {
                collect_map_result_types_from_type_ref(&param.type_ref, result_types);
            }
            collect_map_result_types_from_type_ref(&expr.return_type, result_types);
            collect_map_result_types_from_block(&expr.body, result_types);
        }
        Expression::Taskgroup(expr) => {
            collect_map_result_types_from_type_ref(&expr.result_type, result_types);
            collect_map_result_types_from_block(&expr.body, result_types);
        }
        Expression::Unary(expr) => {
            collect_map_result_types_from_expression(&expr.inner, result_types)
        }
        Expression::Binary(expr) => {
            collect_map_result_types_from_expression(&expr.left, result_types);
            collect_map_result_types_from_expression(&expr.right, result_types);
        }
        Expression::Group(expr) => {
            collect_map_result_types_from_expression(&expr.inner, result_types)
        }
        Expression::Selector(expr) => {
            collect_map_result_types_from_expression(&expr.inner, result_types)
        }
        Expression::Index(expr) => {
            collect_map_result_types_from_expression(&expr.inner, result_types);
            collect_map_result_types_from_expression(&expr.index, result_types);
        }
        Expression::Slice(expr) => {
            collect_map_result_types_from_expression(&expr.inner, result_types);
            if let Some(start) = &expr.start {
                collect_map_result_types_from_expression(start, result_types);
            }
            if let Some(end) = &expr.end {
                collect_map_result_types_from_expression(end, result_types);
            }
        }
        Expression::StructLiteral(expr) => {
            collect_map_result_types_from_type_ref(&expr.type_ref, result_types);
            for field in &expr.fields {
                collect_map_result_types_from_expression(&field.value, result_types);
            }
        }
        Expression::ArrayLiteral(expr) => {
            collect_map_result_types_from_type_ref(&expr.type_ref, result_types);
            for element in &expr.elements {
                collect_map_result_types_from_expression(element, result_types);
            }
        }
        Expression::SliceLiteral(expr) => {
            collect_map_result_types_from_type_ref(&expr.type_ref, result_types);
            for element in &expr.elements {
                collect_map_result_types_from_expression(element, result_types);
            }
        }
        Expression::MapLiteral(expr) => {
            collect_map_result_types_from_type_ref(&expr.type_ref, result_types);
            for pair in &expr.pairs {
                collect_map_result_types_from_expression(&pair.key, result_types);
                collect_map_result_types_from_expression(&pair.value, result_types);
            }
        }
        Expression::Propagate(expr) => {
            collect_map_result_types_from_expression(&expr.inner, result_types)
        }
        Expression::Handle(expr) => {
            collect_map_result_types_from_expression(&expr.inner, result_types);
            collect_map_result_types_from_block(&expr.handler, result_types);
        }
        Expression::Ident(_)
        | Expression::Int(_)
        | Expression::Char(_)
        | Expression::String(_)
        | Expression::Bool(_)
        | Expression::Nil(_)
        | Expression::Error(_)
        | Expression::Missing(_) => {}
    }
}

fn result_type_name(type_: &str) -> String {
    format!("%yar.result.{}", sanitize_label(type_))
}

fn result_struct_literal(type_: &str) -> Result<String, CodegenError> {
    if type_ == "void" {
        return Ok("{ i1, i32 }".to_string());
    }
    Ok(format!("{{ i1, i32, {} }}", llvm_type(type_)?))
}

fn errorable_type(type_: &str) -> String {
    format!("!{type_}")
}

fn arithmetic_op(operator: Kind) -> Result<&'static str, CodegenError> {
    match operator {
        Kind::Plus => Ok("add"),
        Kind::Minus => Ok("sub"),
        Kind::Star => Ok("mul"),
        Kind::Slash => Ok("sdiv"),
        Kind::Percent => Ok("srem"),
        _ => Err(CodegenError::unsupported("binary operator")),
    }
}

fn function_symbol(name: &str) -> String {
    format!("yar.{}", sanitize_label(name))
}

fn host_error_owner(name: &str) -> Option<&'static str> {
    if is_fs_host_intrinsic(name) {
        Some("fs")
    } else if is_process_host_intrinsic(name) {
        Some("process")
    } else if is_env_host_intrinsic(name) {
        Some("env")
    } else if is_net_host_intrinsic(name) {
        Some("net")
    } else {
        None
    }
}

fn host_error_identity(owner: &str, name: &str) -> String {
    if name == "Closed" {
        "error.Closed".to_string()
    } else {
        format!("{owner}.{name}")
    }
}

fn is_fs_host_intrinsic(name: &str) -> bool {
    matches!(
        name,
        "fs.read_file"
            | "fs.write_file"
            | "fs.read_dir"
            | "fs.stat"
            | "fs.mkdir_all"
            | "fs.remove_all"
            | "fs.temp_dir"
            | "fs.open_read_handle"
            | "fs.open_write_handle"
            | "fs.read_handle"
            | "fs.write_handle"
            | "fs.close_handle"
    )
}

fn is_process_host_intrinsic(name: &str) -> bool {
    matches!(name, "process.run" | "process.run_inherit")
}

fn is_env_host_intrinsic(name: &str) -> bool {
    name == "env.lookup"
}

fn is_net_host_intrinsic(name: &str) -> bool {
    matches!(
        name,
        "net.listen"
            | "net.accept"
            | "net.listener_addr"
            | "net.close_listener"
            | "net.connect"
            | "net.read"
            | "net.write"
            | "net.close"
            | "net.local_addr"
            | "net.remote_addr"
            | "net.set_read_deadline"
            | "net.set_write_deadline"
            | "net.resolve"
    )
}

fn interface_table_type_name(name: &str) -> String {
    format!("%yar.iface.table.{}", sanitize_label(name))
}

fn interface_table_literal(method_count: usize) -> String {
    if method_count == 0 {
        return "{ }".to_string();
    }
    format!("{{ {} }}", vec!["ptr"; method_count].join(", "))
}

fn interface_table_global_name(interface_type: &str, concrete_type: &str) -> String {
    format!(
        "yar.iface.table.{}.{}",
        sanitize_label(interface_type),
        sanitize_label(concrete_type)
    )
}

fn interface_adapter_name(interface_type: &str, concrete_type: &str, method: &str) -> String {
    format!(
        "iface.adapter.{}.{}.{}",
        sanitize_label(interface_type),
        sanitize_label(concrete_type),
        sanitize_label(method)
    )
}

fn function_signature_name(function: &FunctionDecl) -> String {
    if let Some(receiver) = &function.receiver {
        return format!(
            "{}.{}",
            method_receiver_base_type(&receiver.type_ref.to_string()),
            function.name
        );
    }
    function.name.clone()
}

fn method_receiver_base_type(type_: &str) -> &str {
    type_.strip_prefix('*').unwrap_or(type_)
}

fn struct_type_name(name: &str) -> String {
    format!("%yar.struct.{}", sanitize_label(name))
}

fn sanitize_label(name: &str) -> String {
    let mut out = String::new();
    for byte in name.bytes() {
        let valid = byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.';
        if valid {
            out.push(byte as char);
        } else {
            write!(&mut out, "_{byte:02x}").unwrap();
        }
    }
    out
}

fn escape_llvm_string(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'\\' => out.push_str("\\5C"),
            b'"' => out.push_str("\\22"),
            0x20..=0x7e => out.push(byte as char),
            _ => write!(&mut out, "\\{byte:02X}").unwrap(),
        }
    }
    out
}

fn expression_name(expression: &Expression) -> &'static str {
    match expression {
        Expression::Ident(_) => "identifier",
        Expression::Int(_) => "integer literal",
        Expression::Char(_) => "character literal",
        Expression::String(_) => "string literal",
        Expression::Bool(_) => "bool literal",
        Expression::Nil(_) => "nil",
        Expression::Error(_) => "error literal",
        Expression::TypeApplication(_) => "type application",
        Expression::Call(_) => "call",
        Expression::FunctionLiteral(_) => "function literal",
        Expression::Taskgroup(_) => "taskgroup",
        Expression::Unary(_) => "unary expression",
        Expression::Binary(_) => "binary expression",
        Expression::Group(_) => "grouped expression",
        Expression::Selector(_) => "selector",
        Expression::Index(_) => "index",
        Expression::Slice(_) => "slice",
        Expression::StructLiteral(_) => "struct literal",
        Expression::ArrayLiteral(_) => "array literal",
        Expression::SliceLiteral(_) => "slice literal",
        Expression::MapLiteral(_) => "map literal",
        Expression::Propagate(_) => "propagate",
        Expression::Handle(_) => "handle",
        Expression::Missing(_) => "missing expression",
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };

    use crate::{
        checker::check_program, lower::lower_package_graph, mono::monomorphize_program,
        package::load_package_graph, parser::parse_file,
    };

    use super::*;

    const CODEGEN_FIXTURES: &[&str] = &[
        "testdata/hello/main.yar",
        "testdata/add/main.yar",
        "testdata/array_bounds/main.yar",
        "testdata/i64/main.yar",
        "testdata/i64_untyped_binary/main.yar",
        "testdata/bool_operators/main.yar",
        "testdata/char_literals/main.yar",
        "testdata/closures/main.yar",
        "testdata/compound_assign/main.yar",
        "testdata/concurrency_basic/main.yar",
        "testdata/concurrency_channels/main.yar",
        "testdata/concurrency_errors/main.yar",
        "testdata/concurrency_fs/main.yar",
        "testdata/concurrency_lifecycle/main.yar",
        "testdata/concurrency_share_safe/main.yar",
        "testdata/deps_local/main.yar",
        "testdata/divide/main.yar",
        "testdata/enum_positional/main.yar",
        "testdata/enums/main.yar",
        "testdata/error_identity/main.yar",
        "testdata/field_visibility/main.yar",
        "testdata/garbage_collection/main.yar",
        "testdata/generics/main.yar",
        "testdata/generics_imports/main.yar",
        "testdata/imports_ok/main.yar",
        "testdata/infinite_for/main.yar",
        "testdata/invalid_string_builder_handle/main.yar",
        "testdata/integer_div_zero/main.yar",
        "testdata/integer_rem_overflow/main.yar",
        "testdata/integer_wrapping/main.yar",
        "testdata/interfaces/main.yar",
        "testdata/maps/main.yar",
        "testdata/maps_keys/main.yar",
        "testdata/match_else/main.yar",
        "testdata/methods/main.yar",
        "testdata/nil_pointer/main.yar",
        "testdata/panic/main.yar",
        "testdata/pointer_escape/main.yar",
        "testdata/pointers/main.yar",
        "testdata/slices/main.yar",
        "testdata/stdlib_conv/main.yar",
        "testdata/stdlib_fs_path/main.yar",
        "testdata/stdlib_io/main.yar",
        "testdata/stdlib_net/main.yar",
        "testdata/stdlib_process_env/main.yar",
        "testdata/stdlib_sort/main.yar",
        "testdata/stdlib_strings/main.yar",
        "testdata/stdlib_strings_ext/main.yar",
        "testdata/stdlib_utf8/main.yar",
        "testdata/open_ended_slice/main.yar",
        "testdata/string_builder/main.yar",
        "testdata/string_ops/main.yar",
        "testdata/structs_and_loops/main.yar",
        "testdata/syntax_surface/main.yar",
        "testdata/testing_basic/main.yar",
        "testdata/testing_fail/main.yar",
        "testdata/unhandled_error/main.yar",
    ];

    #[test]
    fn emits_llvm_for_scalar_control_flow_fixtures() {
        for fixture in CODEGEN_FIXTURES {
            let fixture = *fixture;
            let ir = emit_fixture(fixture);
            assert!(ir.contains("define "), "{fixture}");
            assert!(ir.contains("@yar.main("), "{fixture}");
            assert!(
                ir.contains("define i32 @main(i32 %argc, ptr %argv)"),
                "{fixture}"
            );
            if matches!(
                fixture,
                "testdata/hello/main.yar"
                    | "testdata/add/main.yar"
                    | "testdata/bool_operators/main.yar"
                    | "testdata/char_literals/main.yar"
                    | "testdata/closures/main.yar"
                    | "testdata/compound_assign/main.yar"
                    | "testdata/concurrency_basic/main.yar"
                    | "testdata/concurrency_channels/main.yar"
                    | "testdata/concurrency_errors/main.yar"
                    | "testdata/concurrency_fs/main.yar"
                    | "testdata/deps_local/main.yar"
                    | "testdata/divide/main.yar"
                    | "testdata/enum_positional/main.yar"
                    | "testdata/enums/main.yar"
                    | "testdata/garbage_collection/main.yar"
                    | "testdata/generics/main.yar"
                    | "testdata/generics_imports/main.yar"
                    | "testdata/imports_ok/main.yar"
                    | "testdata/i64_untyped_binary/main.yar"
                    | "testdata/infinite_for/main.yar"
                    | "testdata/integer_wrapping/main.yar"
                    | "testdata/interfaces/main.yar"
                    | "testdata/maps/main.yar"
                    | "testdata/maps_keys/main.yar"
                    | "testdata/match_else/main.yar"
                    | "testdata/methods/main.yar"
                    | "testdata/pointers/main.yar"
                    | "testdata/slices/main.yar"
                    | "testdata/stdlib_conv/main.yar"
                    | "testdata/stdlib_fs_path/main.yar"
                    | "testdata/stdlib_io/main.yar"
                    | "testdata/stdlib_net/main.yar"
                    | "testdata/stdlib_process_env/main.yar"
                    | "testdata/stdlib_sort/main.yar"
                    | "testdata/stdlib_strings/main.yar"
                    | "testdata/stdlib_strings_ext/main.yar"
                    | "testdata/stdlib_utf8/main.yar"
                    | "testdata/open_ended_slice/main.yar"
                    | "testdata/string_builder/main.yar"
                    | "testdata/string_ops/main.yar"
                    | "testdata/structs_and_loops/main.yar"
                    | "testdata/testing_basic/main.yar"
            ) {
                assert!(ir.contains("call void @yar_print("), "{fixture}");
            }
            assert_clang_accepts_ir(&ir, fixture);
        }
    }

    #[test]
    fn lowers_process_env_and_stdio_intrinsics_to_runtime_calls() {
        let ir = emit_project_source(
            "process-limits",
            r#"package main

import "std/env"
import "std/process"
import "std/stdio"

fn main() !i32 {
    host_args := process.args()
    limits := process.limits(1000, 64, 64)?
    cancellation := process.cancellation()
    result := process.run([]str{"tool"}, limits, cancellation)?
    code := process.run_inherit([]str{"tool"}, 1000, cancellation)?
    value := env.lookup("HOME")?
    stdio.eprint(value)
    return result.exit_code + code + len(host_args)
}
"#,
        );
        for expected in [
            "declare void @yar_process_args(ptr)",
            "declare i32 @yar_process_run(ptr, i64, i64, i64, ptr, ptr)",
            "declare i32 @yar_process_run_inherit(ptr, i64, ptr, ptr)",
            "declare i32 @yar_env_lookup(ptr, ptr)",
            "declare void @yar_eprint(ptr, i64)",
            "call void @yar_process_args(ptr %",
            "call i32 @yar_process_run(ptr %process.run.argv",
            "i64 %process.run.timeout",
            "i64 %process.run.max_stdout",
            "i64 %process.run.max_stderr",
            "ptr %process.run.cancellation",
            "call i32 @yar_process_run_inherit(ptr %process.run_inherit.argv",
            "ptr %process.run_inherit.cancellation",
            "call i32 @yar_env_lookup(ptr %",
            "call void @yar_eprint(ptr %",
        ] {
            assert!(ir.contains(expected), "missing {expected:?} in IR:\n{ir}");
        }
        for status in [5, 6, 7] {
            assert!(
                ir.lines().any(|line| {
                    line.contains("icmp eq i32 %process.run.status")
                        && line.ends_with(&format!(", {status}"))
                }),
                "missing process status {status}:\n{ir}"
            );
            assert!(
                !ir.lines().any(|line| {
                    line.contains("icmp eq i32 %env.lookup.status")
                        && line.ends_with(&format!(", {status}"))
                }),
                "environment lookup inherited process-only status {status}:\n{ir}"
            );
        }
    }

    #[test]
    fn lowers_string_builder_handles_as_i64_registry_ids() {
        let ir = emit_fixture("testdata/string_builder/main.yar");
        for expected in [
            "declare i64 @yar_sb_new()",
            "declare void @yar_sb_write(i64, ptr, i64)",
            "declare void @yar_sb_string(i64, ptr)",
            "call i64 @yar_sb_new()",
            "call void @yar_sb_write(i64 %",
            "call void @yar_sb_string(i64 %",
        ] {
            assert!(ir.contains(expected), "missing {expected:?} in IR:\n{ir}");
        }
        assert!(!ir.contains("ptrtoint"), "unexpected ptrtoint in IR:\n{ir}");
        assert!(!ir.contains("inttoptr"), "unexpected inttoptr in IR:\n{ir}");
    }

    #[test]
    fn runtime_aggregate_values_always_cross_explicit_pointers() {
        let ir = emit_fixture("testdata/concurrency_lifecycle/main.yar");
        for declaration in [
            "declare void @yar_taskgroup_wait(ptr, ptr)",
            "declare void @yar_map_keys(ptr, ptr)",
            "declare void @yar_str_concat(ptr, i64, ptr, i64, ptr)",
            "declare void @yar_str_from_byte(i32, ptr)",
            "declare void @yar_to_str_i32(i32, ptr)",
            "declare void @yar_to_str_i64(i64, ptr)",
            "declare void @yar_sb_string(i64, ptr)",
        ] {
            assert!(
                ir.contains(declaration),
                "missing {declaration:?} in IR:\n{ir}"
            );
        }
        assert!(!ir.contains("declare %yar.str @yar_"), "{ir}");
        assert!(!ir.contains("declare %yar.slice @yar_"), "{ir}");
        assert!(
            !ir.lines().any(|line| {
                line.starts_with("declare ") && line.contains("@yar_") && line.contains("%yar.")
            }),
            "runtime aggregate arguments must be pointers:\n{ir}"
        );
    }

    #[test]
    fn lowers_net_intrinsics_to_runtime_calls() {
        let ir = emit_fixture("testdata/stdlib_net/main.yar");
        for expected in [
            "declare i32 @yar_net_listen(ptr, i32, ptr)",
            "declare i32 @yar_net_accept(i64, ptr)",
            "declare i32 @yar_net_listener_addr(i64, ptr)",
            "declare i32 @yar_net_close_listener(i64)",
            "declare i32 @yar_net_connect(ptr, i32, ptr)",
            "declare i32 @yar_net_read(i64, i32, ptr)",
            "declare i32 @yar_net_write(i64, ptr, ptr)",
            "declare i32 @yar_net_close(i64)",
            "declare i32 @yar_net_local_addr(i64, ptr)",
            "declare i32 @yar_net_remote_addr(i64, ptr)",
            "declare i32 @yar_net_resolve(ptr, i32, ptr)",
            "call i32 @yar_net_listen(ptr %",
            "call i32 @yar_net_accept(i64 ",
            "call i32 @yar_net_listener_addr(i64 ",
            "call i32 @yar_net_close_listener(i64 ",
            "call i32 @yar_net_connect(ptr %",
            "call i32 @yar_net_read(i64 ",
            "call i32 @yar_net_write(i64 ",
            "call i32 @yar_net_close(i64 ",
            "call i32 @yar_net_local_addr(i64 ",
            "call i32 @yar_net_remote_addr(i64 ",
            "call i32 @yar_net_resolve(ptr %",
        ] {
            assert!(ir.contains(expected), "missing {expected:?} in IR:\n{ir}");
        }
    }

    #[test]
    fn lowers_channel_equality_to_pointer_comparisons() {
        let ir = emit_source(
            r#"
package main

fn main() i32 {
    ch := chan_new[i32](1)
    same := ch == ch
    different := ch != ch
    if !same || different {
        return 1
    }
    chan_close(ch)
    return 0
}
"#,
        );

        assert!(ir.contains("icmp eq ptr"), "{ir}");
        assert!(ir.contains("icmp ne ptr"), "{ir}");
        assert!(ir.contains("call ptr @yar_chan_new(i64 4, i32 1)"), "{ir}");
        assert!(!ir.contains("chan.elem_size"), "{ir}");
    }

    #[test]
    fn declares_channel_result_types_for_inferred_chan_new_values() {
        let ir = emit_source(
            r#"
package main

fn main() i32 {
    ch := chan_new[str](1)
    chan_send(ch, "value") or |_| {
        return 1
    }
    value := chan_recv(ch) or |_| {
        return 1
    }
    if value != "value" {
        return 1
    }
    return 0
}
"#,
        );

        assert!(ir.contains("%yar.result.void = type { i1, i32 }"), "{ir}");
        assert!(
            ir.contains("%yar.result.str = type { i1, i32, %yar.str }"),
            "{ir}"
        );
    }

    #[test]
    fn passes_checked_taskgroup_element_sizes_without_truncation() {
        let ir = emit_source(
            r#"
package main

fn work() i64 {
    return 1
}

fn main() i32 {
    values := taskgroup []i64 {
        spawn work()
    }
    if len(values) != 1 {
        return 1
    }
    return 0
}
"#,
        );

        assert!(ir.contains("call ptr @yar_taskgroup_new(i64 8)"), "{ir}");
        assert!(!ir.contains("taskgroup.elem_size"), "{ir}");
    }

    #[test]
    fn heap_allocates_address_taken_locals_and_parameters() {
        let ir = emit_source(
            r#"
package main

error Boom
error Unexpected

struct Record {
    value i32
}

fn local_pointer() *i32 {
    value := 7
    return &value
}

fn parameter_pointer(value i32) *i32 {
    return &value
}

fn field_pointer() *i32 {
    record := Record{value: 8}
    return &record.value
}

fn element_pointer() *i32 {
    values := [2]i32{9, 10}
    return &values[1]
}

fn closure_pointer() *i32 {
    make_pointer := fn() *i32 {
        value := 11
        return &value
    }
    return make_pointer()
}

fn fail() !i32 {
    return error.Boom
}

fn error_pointer() *error {
    value := fail() or |err| {
        return &err
    }
    fallback := error.Unexpected
    if value == 0 {
        return &fallback
    }
    return &fallback
}

fn plain(value i32) i32 {
    copy := value
    return copy
}

fn main() i32 {
    return plain(*local_pointer() + *parameter_pointer(1) + *closure_pointer())
}
"#,
        );

        let function_body = |name: &str| {
            let symbol = format!("@{}(", function_symbol(name));
            let symbol_offset = ir
                .find(&symbol)
                .unwrap_or_else(|| panic!("missing {symbol}"));
            let start = ir[..symbol_offset]
                .rfind("define ")
                .unwrap_or_else(|| panic!("missing definition for {symbol}"));
            let end = ir[symbol_offset..]
                .find("\n}\n")
                .map(|offset| symbol_offset + offset + 3)
                .unwrap_or_else(|| panic!("unterminated definition for {symbol}"));
            &ir[start..end]
        };

        for name in [
            "local_pointer",
            "parameter_pointer",
            "field_pointer",
            "element_pointer",
            "closure.0",
        ] {
            let body = function_body(name);
            assert!(body.contains("call ptr @yar_alloc"), "{name}:\n{body}");
            assert!(!body.contains("alloca i32"), "{name}:\n{body}");
        }

        let plain = function_body("plain");
        assert!(plain.contains("alloca i32"), "{plain}");
        assert!(!plain.contains("call ptr @yar_alloc"), "{plain}");

        let error_pointer = function_body("error_pointer");
        assert_eq!(error_pointer.matches("call ptr @yar_alloc").count(), 2);
        assert_eq!(error_pointer.matches("alloca i32").count(), 1);
    }

    #[test]
    fn checks_pointer_dereferences_for_nil() {
        let ir = emit_source(
            r#"
package main

struct Record {
    value i32
}

fn read(pointer *i32) i32 {
    return *pointer
}

fn write(pointer *i32) void {
    *pointer = 1
}

fn read_field(pointer *Record) i32 {
    return pointer.value
}

fn write_field(pointer *Record) void {
    pointer.value = 2
}

fn field_pointer(pointer *Record) *i32 {
    return &pointer.value
}

fn read_nested(pointer **Record) i32 {
    return (*pointer).value
}

fn write_nested(pointer **Record) void {
    (*pointer).value = 3
}

fn nested_field_pointer(pointer **Record) *i32 {
    return &(*pointer).value
}

fn same_pointer(pointer *Record) *Record {
    return pointer
}

fn read_temporary(pointer *Record) i32 {
    return same_pointer(pointer).value
}

fn main() i32 {
    value := 0
    record := Record{value: 0}
    write(&value)
    write_field(&record)
    field := field_pointer(&record)
    if field == nil {
        return 1
    }
    return read(&value) + read_field(&record)
}
"#,
        );

        assert_eq!(
            ir.matches("call void @yar_pointer_check(ptr ").count(),
            12,
            "{ir}"
        );
    }

    #[test]
    fn checks_fixed_array_indexes_for_bounds() {
        let ir = emit_source(
            r#"
package main

fn read(values [2]i32, index i32) i32 {
    return values[index]
}

fn read_i64(values [2]i32, index i64) i32 {
    return values[index]
}

fn write(values [2]i32, index i32) void {
    values[index] = 1
}

fn element_pointer(values [2]i32, index i32) *i32 {
    return &values[index]
}

fn main() i32 {
    values := [2]i32{1, 2}
    write(values, 0)
    return read(values, 0) + read_i64(values, 1) + *element_pointer(values, 0)
}
"#,
        );

        assert_eq!(
            ir.matches("call void @yar_array_index_check(i64 ").count(),
            4,
            "{ir}"
        );
    }

    #[test]
    fn evaluates_compound_assignment_targets_once() {
        let ir = emit_source(
            r#"
package main

struct Record {
    value i32
}

fn next_index(counter *i32) i32 {
    *counter += 1
    return 0
}

fn amount(counter *i32) i32 {
    *counter += 1
    return 2
}

fn main() i32 {
    index_calls := 0
    rhs_calls := 0
    values := [1]i32{1}
    slice := []i32{1}
    records := [1]Record{Record{value: 1}}

    values[next_index(&index_calls)] += amount(&rhs_calls)
    slice[next_index(&index_calls)] += amount(&rhs_calls)
    records[next_index(&index_calls)].value += amount(&rhs_calls)
    return index_calls + rhs_calls
}
"#,
        );

        assert_eq!(ir.matches("call i32 @yar.next_index(").count(), 3, "{ir}");
        assert_eq!(ir.matches("call i32 @yar.amount(").count(), 3, "{ir}");
    }

    #[test]
    fn checks_integer_division_and_remainder_operands() {
        let ir = emit_source(
            r#"
package main

fn arithmetic_i32(left i32, right i32) i32 {
    return left / right + left % right
}

fn arithmetic_i64(left i64, right i64) i64 {
    return left / right + left % right
}

fn main() i32 {
    return 0
}
"#,
        );

        assert_eq!(
            ir.matches("call void @yar_i32_divrem_check(").count(),
            2,
            "{ir}"
        );
        assert_eq!(
            ir.matches("call void @yar_i64_divrem_check(").count(),
            2,
            "{ir}"
        );
    }

    #[test]
    fn preserves_invalid_constant_integer_operations_for_runtime_traps() {
        let ir = emit_source(
            r#"
package main

fn divide_by_zero() i32 {
    return 1 / 0
}

fn remainder_overflow() i64 {
    return (0 - 9223372036854775807 - 1) % (0 - 1)
}

fn typed_value() i64 {
    return 0
}

fn invalid_constant_on_right() i64 {
    return typed_value() + (1 / 0)
}

fn invalid_constant_on_left() i64 {
    return (1 / 0) + typed_value()
}

fn inferred_i64_failure() i64 {
    value := (0 - 9223372036854775807 - 1) % (0 - 1)
    return value
}

fn main() i32 {
    return 0
}
"#,
        );

        assert_eq!(
            ir.matches("call void @yar_i32_divrem_check(").count(),
            1,
            "{ir}"
        );
        assert_eq!(
            ir.matches("call void @yar_i64_divrem_check(").count(),
            4,
            "{ir}"
        );
        assert!(ir.contains(" = sdiv i32 1, 0"), "{ir}");
        assert!(ir.contains(" = srem i64 "), "{ir}");

        let function_body = |name: &str| {
            let symbol = format!("@{}(", function_symbol(name));
            let symbol_offset = ir
                .find(&symbol)
                .unwrap_or_else(|| panic!("missing {symbol}"));
            let start = ir[..symbol_offset]
                .rfind("define ")
                .unwrap_or_else(|| panic!("missing definition for {symbol}"));
            let end = ir[symbol_offset..]
                .find("\n}\n")
                .map(|offset| symbol_offset + offset + 3)
                .unwrap_or_else(|| panic!("unterminated definition for {symbol}"));
            &ir[start..end]
        };
        let invalid_left = function_body("invalid_constant_on_left");
        assert!(
            invalid_left.find("@yar_i64_divrem_check").unwrap()
                < invalid_left.find("@yar.typed_value").unwrap(),
            "{invalid_left}"
        );
        let invalid_right = function_body("invalid_constant_on_right");
        assert!(
            invalid_right.find("@yar.typed_value").unwrap()
                < invalid_right.find("@yar_i64_divrem_check").unwrap(),
            "{invalid_right}"
        );
        let inferred = function_body("inferred_i64_failure");
        assert!(inferred.contains(" = srem i64 "), "{inferred}");
        assert_clang_accepts_ir(&ir, "integer-constant-runtime-traps");
    }

    #[test]
    fn covers_all_testdata_main_fixtures() {
        let root = repo_root();
        let mut discovered = Vec::new();
        collect_main_yar_fixtures(&root, &root.join("testdata"), &mut discovered);
        discovered.sort();

        let mut covered = CODEGEN_FIXTURES
            .iter()
            .map(|fixture| (*fixture).to_string())
            .collect::<Vec<_>>();
        covered.sort();

        assert_eq!(covered, discovered);
    }

    fn emit_fixture(fixture: &str) -> String {
        emit_fixture_result(fixture).unwrap_or_else(|err| panic!("{fixture}: {err}"))
    }

    fn emit_fixture_result(fixture: &str) -> Result<String, CodegenError> {
        let root = repo_root();
        let path = root.join(fixture);
        let (graph, diagnostics) = load_package_graph(&path, false).unwrap();
        assert_eq!(diagnostics, Vec::new(), "{fixture} load");
        let (lowered, diagnostics) = lower_package_graph(&graph);
        assert_eq!(diagnostics, Vec::new(), "{fixture} lower");
        let (mono, diagnostics) = monomorphize_program(&lowered);
        assert_eq!(diagnostics, Vec::new(), "{fixture} mono");
        let (info, diagnostics) = check_program(&mono);
        assert_eq!(diagnostics, Vec::new(), "{fixture} check");
        emit_llvm(&mono, &info)
    }

    fn emit_project_source(label: &str, source: &str) -> String {
        static NEXT_PROJECT: AtomicU64 = AtomicU64::new(0);

        let id = NEXT_PROJECT.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("yar-codegen-{label}-{}-{id}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("main.yar");
        fs::write(&entry, source).unwrap();

        let result = {
            let (graph, diagnostics) = load_package_graph(&entry, false).unwrap();
            assert_eq!(diagnostics, Vec::new(), "{label} load");
            let (lowered, diagnostics) = lower_package_graph(&graph);
            assert_eq!(diagnostics, Vec::new(), "{label} lower");
            let (mono, diagnostics) = monomorphize_program(&lowered);
            assert_eq!(diagnostics, Vec::new(), "{label} mono");
            let (info, diagnostics) = check_program(&mono);
            assert_eq!(diagnostics, Vec::new(), "{label} check");
            emit_llvm(&mono, &info).unwrap()
        };

        fs::remove_dir_all(&dir).unwrap();
        result
    }

    fn emit_source(src: &str) -> String {
        let (program, diagnostics) = parse_file("<test>", src);
        assert_eq!(diagnostics, Vec::new(), "parse");
        let (info, diagnostics) = check_program(&program);
        assert_eq!(diagnostics, Vec::new(), "check");
        emit_llvm(&program, &info).unwrap()
    }

    fn collect_main_yar_fixtures(root: &Path, dir: &Path, fixtures: &mut Vec<String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect_main_yar_fixtures(root, &path, fixtures);
            } else if path.file_name().is_some_and(|name| name == "main.yar") {
                fixtures.push(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }

    fn assert_clang_accepts_ir(ir: &str, fixture: &str) {
        if Command::new("clang").arg("--version").output().is_err() {
            return;
        }

        let dir = std::env::temp_dir().join(format!(
            "yar-rust-codegen-{}-{}",
            std::process::id(),
            fixture.replace(['/', '.'], "-")
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let ir_path = dir.join("main.ll");
        let object_path = dir.join("main.o");
        std::fs::write(&ir_path, ir).unwrap();
        let output = Command::new("clang")
            .arg("-c")
            .arg(&ir_path)
            .arg("-o")
            .arg(&object_path)
            .output()
            .unwrap_or_else(|err| panic!("run clang for {fixture}: {err}"));
        assert!(
            output.status.success(),
            "clang rejected {fixture} IR\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("crate is nested under crates/yar-compiler")
            .to_path_buf()
    }
}
