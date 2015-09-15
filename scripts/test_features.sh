#List of features to test
features=""

#Features that will always be activated
required_features="strict"


#Find features
walking_features=false
current_dependency=""

while read -r line || [[ -n "$line" ]]; do
	if [[ "$line" == "[features]" ]]; then
		walking_features=true
	elif [[ $walking_features == true ]] && [[ "$line" == "#internal" ]]; then
		walking_features=false
	elif [[ $walking_features == true ]] && echo "$line" | grep -E "^\[.*\]" > /dev/null; then
		walking_features=false
	elif [[ $walking_features == true ]] && echo "$line" | grep -E ".*=.*" > /dev/null; then
		feature="$(echo "$line" | cut -f1 -d"=")"
		feature="$(echo -e "${feature}" | tr -d '[[:space:]]')"
		if [[ "$feature" != "default" ]]; then
			echo "found feature '$feature'"
			features="$features $feature"
		fi
	elif echo "$line" | grep -E "^\[dependencies\..*\]" > /dev/null; then
		current_dependency="$(echo "$line" | sed 's/.*\[dependencies\.\([^]]*\)\].*/\1/g')"
	elif [[ "$line" == "#feature" ]] && [[ "$current_dependency" != "" ]]; then
		echo "found dependency feature '$current_dependency'"
		features="$features $current_dependency"
	fi
done < "Cargo.toml"

#Test without any optional feature
echo compiling with --no-default-features --features "$required_features"
cargo build --no-default-features --features "$required_features"

#Isolated test of each optional feature
for feature in $features; do
	echo compiling with --no-default-features --features "\"$feature $required_features\""
	cargo build --no-default-features --features "$feature $required_features"
done
