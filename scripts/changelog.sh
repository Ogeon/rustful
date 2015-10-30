current_version="$(cargo read-manifest | sed 's/.*"version":"\([^"]\+\)".*/\1/g')"
current_date=

echo -e "# Changelog\n" > CHANGELOG.md
echo -e "## Version $current_version - $(date +%F)\n" >> CHANGELOG.md

pulls=()
issues=()

git log --pretty="%an<%ae>;%H;%ad;%s" --date=short |
{
	while read line; do
		if [[ $line =~ Homu\<homu@barosl.com\>\;.* ]]; then
			parts="$(echo "$line" | sed 's/.*;\([^;]*\);.*;Auto merge of #\([0-9]*\)*/\1 \2/g')"
			parts=($parts)
			description="$(git log -1 --pretty=format:%b ${parts[0]})"
			header="$(echo "$description" | head -n 1)"

			fixes="$(echo "$description" | grep -iEo "(close|closes|closed|fix|fixes|fixed|resolve|resolves|resolved) #[0-9]+" | sed 's/.* #\([0-9]*\)/\1/g')"

			issues+=("$fixes")
			pulls+=("${parts[1]}")

			fixes="$(echo "$fixes" | sed ':a;N;$!ba;s/\n/, /g' | sed 's/\([0-9]\+\)/[#\1][\1]/g')"

			entry=" * [#${parts[1]}][${parts[1]}]: $header."

			if [[ "$fixes" != "" ]]; then
				echo "$entry Closes $fixes." >> CHANGELOG.md
			else
				echo "$entry"  >> CHANGELOG.md
			fi
		elif [[ $line =~ .*\;.*\;.*\;Version\ [0-9]+\.[0-9]+\.[0-9]+$ ]]; then
			parts="$(echo "$line" | sed 's/.*;.*;\(.\+\);Version \(.*\)/\1 \2/g')"
			parts=($parts)
			echo -e "\n## Version ${parts[1]} - ${parts[0]}\n" >> CHANGELOG.md
		fi
	done

	for id in ${pulls[@]}; do
		echo "[$id]: https://github.com/Ogeon/rustful/pull/$id" >> CHANGELOG.md
	done

	for id in ${issues[@]}; do
		echo "[$id]: https://github.com/Ogeon/rustful/issues/$id" >> CHANGELOG.md
	done
}
