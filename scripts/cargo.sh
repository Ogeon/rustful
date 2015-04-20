if [ "$TRAVIS_RUST_VERSION" = "nightly" ]; then
	cargo $@ --features nightly
else
	cargo $@
fi