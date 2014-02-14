.PHONY: all

all:
	rustc --opt-level=3 src/lib.rs

test:
	rustc --opt-level=3 --test src/lib.rs -o rustful-test
	./rustful-test --test --bench