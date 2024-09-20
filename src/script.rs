use rhai::{CallFnOptions, FuncArgs, Scope, Variant, AST};

use crate::rhai_hal::{self};

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
    pub fn new() -> ScriptRunner {
        let mut runner = ScriptRunner {
            engine: rhai::Engine::new(),
            base_state: rhai::Map::new(),
            script: ScriptInstance {
                ast: AST::empty(),
                scope: Scope::new(),
                state: rhai::Map::new().into(),
            },
        };
        runner.engine.build_type::<rhai_hal::timer::CallTimer>();
        runner.engine.build_type::<rhai_hal::ledc::PwmController>();
        runner
    }

    pub fn insert_builtin(&mut self, key: &str, val: impl rhai::Variant + Clone) {
        self.base_state.insert(key.into(), rhai::Dynamic::from(val));
    }

    pub fn recompile(&mut self, new_script: &str) -> anyhow::Result<()> {
        // self.script.state
        self.script.state = self.base_state.clone().into();
        self.script.scope.clear();
        self.script.ast = self
            .engine
            .compile_with_scope(&self.script.scope, new_script)?;

        let _ = self.call::<rhai::Dynamic>("init", ());

        Ok(())
    }

    pub fn call<T: Variant + Clone>(&mut self, name: &str, args: impl FuncArgs) -> T {
        let ScriptInstance { ast, scope, state } = &mut self.script;
        self.engine
            .call_fn_with_options(
                CallFnOptions::new()
                    .eval_ast(true)
                    .rewind_scope(true)
                    .bind_this_ptr(state),
                scope,
                ast,
                name,
                args,
            )
            .unwrap()
    }
}
