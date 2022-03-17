#!/bin/env sh
set -euo pipefail

#
echo \
"
# This file is updated by \`update-toolchain.sh\`
# We aim to be around 1-2 rust releases behind in order
# to get people the time to update their toolchains properly.
# By enforcing this, we can also make full use of the features
# provided by the current channel.
"

TESTVAR=new; cat ./rust-toolchain | \
sed -e "/channel/s/\".*\"/\"$TESTVAR\"/"


