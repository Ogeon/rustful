LIBS=-L lib

.PHONY: both rustful macros deps test docs examples

both: rustful macros

rustful:
	rm -f lib/librustful-*
	rustc $(LIBS) --opt-level=3 src/lib.rs --out-dir lib/

macros:
	rm -f lib/librustful_macros-*
	rustc $(LIBS) --opt-level=3 src/macros.rs --out-dir lib/


deps:
	@if [ -e .git ] ; then \
		git submodule init; \
		git submodule sync; \
		git submodule update; \
	fi
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
	rustdoc $(LIBS) src/macros.rs

examples:
	rustc $(LIBS) examples/hello_world/main.rs -o examples/hello_world/main
	rustc $(LIBS) examples/post/main.rs -o examples/post/main