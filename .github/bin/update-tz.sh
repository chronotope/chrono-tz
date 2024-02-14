#!/bin/sh

main() {
    auth_header="$(git config --local --get http.https://github.com/.extraheader)"
    git submodule sync --recursive
    git -c "http.extraheader=$auth_header" -c protocol.version=2 \
        submodule update --init --force --recursive

    set -xeu

    cd tz

    git fetch --tags --quiet origin
    orig_tag="$(git describe)"

    git checkout main
    git pull --ff-only

    new_tag="$(git describe --abbrev=0)"

    if [ "$new_tag" != "$orig_tag" ]; then
        tz_branch="update-tz-${new_tag}"
        if (git branch -r | grep "origin/$tz_branch")>/dev/null 2>&1 ; then
            echo "::set-output name=did_update::already_done"
            return 0
        fi
        git reset --hard "$new_tag"
        cd ..
        git add tz
        git checkout -b "$tz_branch"
        git config user.name "Brandon W Maister"
        git config user.email "quodlibetor@gmail.com"
        msg="Update tz $orig_tag -> $new_tag"
        git commit -m "$msg"
        git push -u origin "$tz_branch"
        echo "::set-output name=did_update::yes"
        echo "::set-env name=PULL_REQUEST_FROM_BRANCH::${tz_branch}"
        echo "::set-env name=PULL_REQUEST_TITLE::$msg"
        echo "::set-env name=PULL_REQUEST_BODY=''"
        echo "::set-env name=PULL_REQUEST_REVIEWERS='quodlibetor djzin'"
    else
        echo "::set-output name=did_update::no"
    fi
}

main
