use std::str::FromStr;

use crate::rhai_hal::{self};
use crate::rpc::{MessageRecycler, ResponseMessage, ResponseTag};
use rhai::packages::Package;
use rhai::{CallFnOptions, EvalAltResult, FuncArgs, ImmutableString, Scope, Variant, AST};
use rhai_rand::RandomPackage;
use thingbuf::mpsc::blocking::Sender;

#[repr(transparent)]
pub struct LovenseArgs<'a>(&'a str);

impl<'a> LovenseArgs<'a> {
    pub fn new(val: &'a str) -> Option<Self> {
        let msg_end = val.find(';')?;
        Some(LovenseArgs(&val[..msg_end]))
    }
}

impl<'a> FuncArgs for LovenseArgs<'a> {
    fn parse<ARGS: Extend<rhai::Dynamic>>(self, args: &mut ARGS) {
        let mut array = rhai::Array::with_capacity(4);
        for s in self.0.split_terminator(':') {
            array.push(ImmutableString::from_str(s).unwrap().into());
        }
        args.extend([array.into()]);
    }
}

pub struct ScriptInstance {
    ast: rhai::AST,
    scope: rhai::Scope<'static>,
    state: rhai::Dynamic,
}

pub struct ScriptRunner {
    engine: rhai::Engine,
    script: ScriptInstance,
    base_state: rhai::Map,
}

impl ScriptRunner {
    pub fn new(log_tx: Sender<ResponseMessage, MessageRecycler>) -> ScriptRunner {
        let mut runner = ScriptRunner {
            engine: rhai::Engine::new(),
            base_state: rhai::Map::new(),
            script: ScriptInstance {
                ast: AST::empty(),
                scope: Scope::new(),
                state: rhai::Map::new().into(),
            },
        };
        runner.engine.set_max_strings_interned(0);
        runner
            .engine
            .register_global_module(RandomPackage::new().as_shared_module());
        rhai_hal::register(&mut runner.engine);
        runner.engine.on_print(move |s| {
            println!("{s}");
            if let Ok(mut slot) = log_tx.try_send_ref() {
                slot.buffer.extend_from_slice(s.as_bytes());
                slot.tag = ResponseTag::Log;
            }
            // log::info!(target: "rhai", "{s}");
        });
        runner
    }

    pub fn insert_builtin(&mut self, key: &str, val: impl rhai::Variant + Clone) {
        self.base_state.insert(key.into(), rhai::Dynamic::from(val));
    }

    pub fn recompile(&mut self, new_script: &str) -> anyhow::Result<()> {
        // self.script.state
        self.script.state = self.base_state.clone().into();
        self.script.scope.clear();
        self.script.ast = AST::empty();
        self.script.ast = self
            .engine
            .compile_with_scope(&self.script.scope, new_script)?;

        let _ = self.call::<rhai::Dynamic>("init", ());

        Ok(())
    }

    pub fn call<T: Variant + Clone>(
        &mut self,
        name: &str,
        args: impl FuncArgs,
    ) -> Result<T, Box<EvalAltResult>> {
        let ScriptInstance { ast, scope, state } = &mut self.script;
        self.engine.call_fn_with_options(
            CallFnOptions::new()
                .eval_ast(true)
                .rewind_scope(true)
                .bind_this_ptr(state),
            scope,
            ast,
            name,
            args,
        )
    }
}
