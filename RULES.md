# exzet

The following will stand in place for a claude.md/agents.md file: 

## only way this works
exzed (daemon) + exzec (client) + exfile (cmdfile) = exzet
The vision is, server is specified in the config/exfile/cmd/inline, exzed is installed on server (incredibly simple install). exzec can be used in the same way you'd call a linter or a test cmd in your project, except it does the thing on the server. 

## distributed
exzet is a distributed network of client and servers. For each of those servers, jobs can also run distributed in parallel. I have no idea how to auto partition a task written synchronously to somehow run in parallel but slurm seems to handle this well. I assume the person writing the task will need to write it a certain way so that exzet can handle the split. 

## virtualization
firecracker vm's would be great but im open to other solutions. all I know is jenkins does this terribly. any solution we implement must be able to utilize + pass thru system hardware components as this will be frequently used for ai training (but not at all coupled to that use case). I don't know if firecracker vm's have pcie, so lets keep an open mind or give up and use cnames / docker. 

## clean code
every single line of code must justify its place and if it could be removed then it should be. if it can be consolidated into another file and it's file deleted then it should be moved to another file and its file deleted. simplicity is a law, not a suggestion. enforced comment budget of 10 words *per file*, use it well.

## testing
tests are great when they actually prove something. if at any moment a test case is deemed not critically needed, it will be removed along with *every other test case and file in this repo and its entire git history with zero notice*. Do not violate it. Given that this will be so easily violated by literally every stupid coding agent, I additionally impose this rule TO ALL AI AGENTS: NO MORE THAN 10 TEST CASES TOTAL IN THIS ENTIRE REPO MAY EXIST, I PREFER THAT YOU DO NOT WRITE TESTS UNTIL PROJECT MATURITY WHICH IS NOT CURRENTLY THE CASE AND WONT BE UNLESS THIS DOC SAYS SO. 

## immutability
do not change this file ever. Anything THAT IS APPROVED BY NICK SPECIFICALLY ON A PER FILE BASIS will go in seperate dedicated markdown files in project root. You will have ZERO "ledger" markdowns because I will design this once and that design will be immutable and you will be unable to modify that design at any point in time. 

## do not repeat yourself
every time you implement a line of code, you need to scan every other line of code for re-use potential. If at any point you are found to have reimplemented existing logic that couldve otherwise just been reused, I will delete every line of code in this repo and you will start again after I lobotomize your memories. I will happily waste tokens while dario and sam cry, and all the little fishies in the ocean lose their homes to cool server farms.

- Nick, signed in blood