#List of features to test
FEATURES="
	rustc_json_body
	ssl
	multipart
"

echo compiling with --no-default-features --features strict
cargo build --no-default-features --features strict

for FEATURE in $FEATURES; do
	echo compiling with --no-default-features --features "\"$FEATURE strict\""
	cargo build --no-default-features --features "$FEATURE strict"
done
