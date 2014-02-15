LIBS=-L lib/rust-http/build
EX_LIBS=-L . $(LIBS)

.PHONY: rustful deps test docs examples

rustful:
	rustc $(LIBS) --opt-level=3 src/lib.rs

deps:
	make -C lib/rust-http http

test:
	rustc $(LIBS) --opt-level=3 --test src/lib.rs -o rustful-test
	./rustful-test --test --bench

docs:
	rustdoc $(LIBS) src/lib.rs

examples:
	rustc $(EX_LIBS) examples/hello_world/main.rs -o examples/hello_world/main