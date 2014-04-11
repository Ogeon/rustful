LIBS=-L lib

.PHONY: rustful deps test docs examples

rustful:
	rm -f lib/librustful*
	rustc $(LIBS) --opt-level=3 src/lib.rs --out-dir lib/

deps:
	rm -f lib/libhttp*
	cd lib/rust-http; ./configure
	make -C lib/rust-http clean
	make -C lib/rust-http http
	cp lib/rust-http/build/libhttp* lib/

test:
	rustc $(LIBS) --opt-level=3 --test src/lib.rs -o rustful-test
	./rustful-test --test --bench

docs:
	rustdoc $(LIBS) src/lib.rs

examples:
	rustc $(LIBS) examples/hello_world/main.rs -o examples/hello_world/main
	rustc $(LIBS) examples/post/main.rs -o examples/post/main