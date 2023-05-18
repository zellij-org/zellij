WHAT IS THIS?
Radicle is a peer-to-peer network for code collaboration, built on top of Git. It is gossipy, secure, local-first, and is currently in Alpha, developing rapidly. It could be a backup for Github or other centralized code forges, and is moving towards becoming a full replacement. See radicle.xyz for a more complete introduction.

seednode.link is an unaffiliated service using Radicle that aims to provide useful functionality, a learning sandbox, and will gradually show more and more of the potential of Radicle. Currently it is bare bones, but plans to expand to become more fully featured, including a web service, especially if there is strong interest or usage. For a current example, see the official Radicle seednode web interface at https://app.radicle.xyz/

Both are free to use. Radicle uses MIT and Apache 2.0 licenses. seednode.link runs on Arch Linux and FOSS wherever possible.

HOW TO USE RAD?
seednode.link is set to automatically track, or "host" any publicly facing Radicle project repos that connect directly to it. To do this, a user would have a regular Git repo, then use Radicle to push to the Radicle network.
In brief, this means...
* Installing Radicle or using provided binaries (currently updated every few days at https://files.radicle.xyz/latest/)
* `rad auth` (creates a unique Network ID [NID] that signs rad repos and is used in Radicle routing information)
* `rad init` (in a regular Git repo, creates a Repository Identifier [a fingerprint known as a RID] for a particular user's NID) 
* `git push rad <branch>` (this pushes to a user's local Radicle Storage [a "remote" which holds copies of Radicle-enabled Git repos] from the repo's workspace. This readies the repo for the network.)
* `radicle-node --connect z6MkixDzQ7GZsuNwFueAeTHoCNBXr2zQ1zA11jaVTqx9rMeX@seednode.link:10192` (seednode.link will track your project and seed it automatically)

In addition to the above there are developing options for issues, patches, tracking of other people's RIDs and NIDs, and more.

WHY THIS PROJECT?
A handful of projects have been selected to be tracked on seednode.link because we wanted to see them available resiliently, but in no way are these RIDs canonical. Because there is no central authority, each NID (user) will have its own version of a repo, and "canonical" would be only by convention. Therefore, each project that wishes to participate in Radicle could include information about this topic in their repo as a reference point.

WHO PAYS?
At this time seednode.link is paid for and maintained by operating volunteers. In the future, addresses for contribution to keep the project going may be published.

MORE?
Radicle official project chat: https://radicle.zulipchat.com
Radicle Improvement Proposals (RIPs) showcase the elegant design of the system: https://app.radicle.xyz/seeds/seed.radicle.xyz/rad:z3trNYnLWS11cJWC6BbxDs5niGo82
Unofficial Matrix Chat: #radicle:matrix.org
