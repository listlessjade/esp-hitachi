fn main() {
    embuild::espidf::sysenv::output();
    build_data::set_GIT_BRANCH();
    build_data::set_GIT_COMMIT();
    build_data::set_GIT_DIRTY();
    build_data::set_BUILD_TIMESTAMP();
    build_data::no_debug_rebuilds();
    build_data::set_RUSTC_VERSION();
}
