has_lib_changed = !git.modified_files.grep(/src/).empty?
has_examples_changed = !git.modified_files.grep(/examples/).empty?
has_build_changed = !git.modified_files.grep(/build/).empty?
has_readme_changed = !git.modified_files.include?("README.md")
has_hello_world_changed = !git.modified_files.include?("examples/hello_world.rs")

if has_hello_world_changed && !has_readme_changed
	warn "Please make sure that README.md is in sync with hello_world.rs."
end

marked_other = git.pr_title.include?("[other]")
if !has_lib_changed && !has_examples_changed && !has_build_changed && !marked_other
	fail "Please add [other] to the PR title if the library or examples wasn't changes."
end

marked_wip = git.pr_title.downcase().include?("[wip]")
if marked_wip
	warn "This PR is a work in progress. It may change at any time."
end
