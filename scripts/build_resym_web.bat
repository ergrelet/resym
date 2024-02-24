REM Setup build environment
call .\scripts\setup_web.bat

set CRATE_NAME=resym
set LIB_NAME=resym_web

REM Build resym-web
set RUSTFLAGS=--cfg=web_sys_unstable_apis -C target-feature=+atomics,+bulk-memory,+mutable-globals
rustup run nightly-2024-02-24 wasm-pack build --target web %CRATE_NAME% -- --no-default-features -Z build-std=panic_abort,std

REM Copy web resources next to the output
copy %CRATE_NAME%\resources\web\* %CRATE_NAME%\pkg\
copy %CRATE_NAME%\resources\resym_96.ico %CRATE_NAME%\pkg\favicon.ico

ECHO "Finished '%CRATE_NAME%\pkg\%LIB_NAME%_bg.wasm'"
