if [ "$TRAVIS_RUST_VERSION" = "stable" ]; then
	curl https://raw.githubusercontent.com/ogeon/travis-doc-upload/master/travis-doc-upload.sh | sh
fi