if [ "$TRAVIS_PULL_REQUEST" == "false" ] && [ "$TRAVIS_BRANCH" == "master" ] && [ -e doc ]; then
echo Starting gh-pages upload...

# Edit docs to only keep rustful related documents
grep -v "'rustful" doc/search-index.js | sed -ne "s/^searchIndex\['\([a-z_\-]*\).*/doc\/\1 doc\/src\/\1/p" | xargs rm -r
sed -ni "/'rustful\|^var\|^init/p" doc/search-index.js

cp -r doc $HOME/doc

# Go to home and setup git
cd $HOME
git config --global user.email "travis@travis-ci.org"
git config --global user.name "Travis"

# Clone gh-pages branch
git clone --quiet --branch=gh-pages https://${GH_TOKEN}@github.com/Ogeon/rustful.git gh-pages > /dev/null

# Copy over the documentation
cd gh-pages
rm -rf doc
cp -r $HOME/doc .

# Add, commit and push files
git add -f --all .
git commit -m "Update docs from Travis build $TRAVIS_BUILD_NUMBER"
git push -fq origin gh-pages > /dev/null

echo Done uploading documentation to gh-pages!
fi