if [ "$TRAVIS_RUST_VERSION" != "nightly" ]; then
	curl https://raw.githubusercontent.com/ogeon/travis-doc-upload/master/travis-doc-upload.sh | sh
fi