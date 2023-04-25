headers:
	cbindgen src/lib.rs -l c > ones/ones/Classes/rust-ios.h
build:
	cargo lipo --release
pod: headers build
	rm ones/ones/lib/libones_ios_arm64.a & rm ones/ones/lib/libones_ios_x86.a
	mv target/aarch64-apple-ios/release/libones_ios.a ones/ones/lib/libones_ios_arm64.a
	mv target/x86_64-apple-ios/release/libones_ios.a ones/ones/lib/libones_ios_x86.a
build-platform:
	cargo build --release
buildc: build-platform
	gcc c/main.c -I./ones/ones/Classes -L./target/release -lones_ios -o test