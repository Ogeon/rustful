if [ "$TRAVIS_PULL_REQUEST" == "false" ]; then
echo Starting gh-pages upload...

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