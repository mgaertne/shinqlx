.. _native_plugins:
.. currentmodule:: shinqlx

#######################
Built-in Rust plugins
#######################

.. shinqlx-only::

   The whole page: minqlx has no built-in Rust plugins.

shinqlx ships with ports of the default minqlx plugins that are written in Rust and compiled into the server mod.
They are drop-in replacements: they keep the commands, the cvars and the database keys of the Python plugins, so the
data of an existing server is kept, and other plugins that read these keys, for example the ``alias`` and ``ip``
plugins from tjone270, keep working.

Loading
=======

A plugin is selected with a prefix, see :func:`load_plugin`:

* ``p!name`` loads the Python plugin ``name`` from the plugins directory,
* ``r!name`` loads the built-in Rust plugin ``name``,
* ``name`` without a prefix loads the Python plugin, and the built-in one only if there is no Python file of that name.

The prefixes work in ``qlx_plugins``, with ``!load`` and with ``!reload``. A plugin is known by its bare name, so
``!unload ban`` unloads the Rust plugin ``ban`` as well. ``p!name`` and ``r!name`` cannot be loaded at the same time.

.. code-block:: none

   set qlx_plugins "r!plugin_manager, r!permission, r!essentials, r!ban, r!silence, r!motd, r!names, r!clan, r!log, r!workshop"

Plugins that replace a default plugin, for example ``myban`` or ``myessentials``, unload the plugin they replace when
they are loaded. That works the same for the built-in plugins.

.. note::
   Texts shown to players keep the wording of the Python plugins. Where a built-in plugin deviates from its original, it
   is mentioned in the section of the plugin.

Commands that act on a player take a client id, a Steam ID or a part of the player's name. If a name matches nobody or
several players, the caller is told so and no further command processing takes place.

plugin_manager
==============

Loads and unloads plugins while the server is running. All commands need permission level 5.

========================= ==================================================================
command                   description
========================= ==================================================================
``!load <plugin>``        Loads a plugin. Takes the prefixes described above.
``!unload <plugin>``      Unloads a plugin. Takes the bare name.
``!reload <plugin>``      Reloads a plugin. Takes the prefixes, so it can switch the source.
``!loadall``              Loads all plugins of ``qlx_plugins``.
``!unloadall``            Unloads all plugins except the manager itself.
``!reloadall``            Reloads all plugins.
========================= ==================================================================

permission
==========

Gets and sets the permission levels of players, independent of the database used.

========================= ======= ====================================================
command                   level   description
========================= ======= ====================================================
``!setperm <id> <level>`` 5       Sets the permission level of a player.
``!getperm <id>``         5       Shows the permission level of a player.
``!myperm``               0       Shows your own permission level, in game only.
========================= ======= ====================================================

The owner (``qlx_owner``) always has all permissions, whatever level is stored for them in the database.

ban
===

Bans players temporarily, also across server restarts, and optionally for leaving too many games. Hooks ``game_start`` and
``game_end``.

=========================================== ======= =======================================================
command                                     level   description
=========================================== ======= =======================================================
``!ban <id> <length> <unit> [reason]``      2       Bans a player for the given time, e.g. ``1 hour``.
``!unban <id>``                             2       Lifts all active bans of a player.
``!checkban <id>``                          0       Shows the active bans of a player.
``!forgive <id> [leaves_to_forgive]``       2       Forgives leaves of a player.
=========================================== ======= =======================================================

================================ ========= ====================================================================
cvar                             default   description
================================ ========= ====================================================================
``qlx_leaverBan``                ``0``     Ban players who leave too many games.
``qlx_leaverBanThreshold``       ``0.63``  Share of completed games below which a player is banned.
``qlx_leaverBanWarnThreshold``   ``0.78``  Share of completed games below which a player is warned.
``qlx_leaverBanMinimumGames``    ``15``    Number of games before the share is taken into account.
================================ ========= ====================================================================

Compared to the Python plugin, an invalid or non-positive length answers with the usage instead of doing nothing,
and the expiry time is taken from the stored score.

Other plugins can ask the plugin through ``self.plugins["ban"]``, as with the Python plugin:
``is_banned(steam_id)`` returns the expiry as :class:`datetime.datetime` and the reason of the longest active ban or
``None``, ``leave_status(steam_id)`` returns the action (``"warn"``, ``"ban"`` or ``None``) and the share of
completed games or ``None``, and ``warn_player(player, ratio)`` tells a player about their leaves.

silence
=======

Mutes players temporarily, also across server restarts.

=========================================== ======= =======================================================
command                                     level   description
=========================================== ======= =======================================================
``!silence <id> <length> <unit> [reason]``  2       Silences a player for the given time.
``!unsilence <id>``                         2       Lifts all active silences of a player.
``!checksilence <id>``                      0       Shows the active silences of a player.
=========================================== ======= =======================================================

Other plugins can ask the plugin through ``self.plugins["silence"]``: ``is_silenced(steam_id)`` returns the expiry as
:class:`datetime.datetime`, the score and the reason of the longest active silence, or ``None``.

essentials
==========

The everyday commands for the admins of a server, and the bookkeeping of players, IP addresses and votes. Besides
the commands it keeps the names, IP addresses and the time of the last visit of every player in the database, which
plugins like ``alias`` and ``ip`` read.

========================================== ======= =============================================================
command                                    level   description
========================================== ======= =============================================================
``!id [part_of_name] ...``                 1       Lists the ids of the players matching the names.
``!players``                               1       Lists the connected players with their ids, Steam IDs and IPs.
``!disconnects``, ``!dcs``                 1       Shows the most recent player disconnects.
``!commands``, ``!cmds``                   2       Shows the most recent commands that were executed.
``!db <key>``                              5       Prints a value of the database.
``!seen <steam_id>``                       0       Tells when a player was last seen.
``!time [timezone_offset]``                0       Tells the current time.
``!help``, ``!about``, ``!version``        0       Tells the version of shinqlx.
``!rcon <command>``                        5       Runs a console command, with the output sent to you.
``!shuffle``                               1       Shuffles the teams.
``!pause``, ``!timeout``                   1       Pauses the game.
``!unpause``, ``!timein``                  1       Resumes the game.
``!slap <id> [damage]``                    2       Slaps a player.
``!slay <id>``                             2       Kills a player.
``!sound <path>``, ``!music <path>``       1       Plays a sound or music for all who enabled sounds.
``!stopsound``, ``!stopmusic``             1       Stops the sound or music.
``!sounds``                                0       Toggles whether you hear sounds and music.
``!kick <id> [reason]``                    2       Kicks a player.
``!kickban <id>``, ``!tempban <id>``       2       Kicks a player for the remainder of the map.
``!yes``, ``!no``                          2       Passes or vetoes the current vote.
``!random <limit>``                        1       Announces a random number.
``!switch <id> <id>``                      1       Swaps the teams of two players.
``!red <id>``, ``!blue <id>``              1       Moves a player to a team.
``!free <id>``                             1       Moves a player to the free team.
``!spectate <id>``, ``!spec``              1       Moves a player to the spectators.
``!addmod <id>``, ``!addadmin <id>``       5       Gives a player moderator or admin status.
``!demote <id>``                           5       Takes all permissions away from a player.
``!mute <id>``, ``!unmute <id>``           1       Mutes or unmutes a player.
``!lock [team]``, ``!unlock [team]``       1       Locks or unlocks a team, or all teams.
``!allready``                              2       Forces all players to ready up.
``!abort``                                 2       Sends a running game back to warm-up.
``!map <name> [factory]``, ``!changemap``  2       Changes the map.
``!teamsize <size>``, ``!ts``              2       Sets the team size.
``!mappool``, ``!maps``, ``!maplist``      0       Shows the maps that can be voted, also via ``client_command``.
========================================== ======= =============================================================

========================== ========= =================================================================================
cvar                       default   description
========================== ========= =================================================================================
``qlx_votepass``           ``1``     Pass a vote automatically after 29 seconds if it is leading.
``qlx_votepassThreshold``  ``0.33``  Share of the players in the teams that has to have voted for that to happen.
``qlx_teamsizeMinimum``    ``1``     Smallest team size that can be voted.
``qlx_teamsizeMaximum``    ``8``     Largest team size that can be voted.
``qlx_enforceMappool``     ``0``     Only allow votes for the maps and factories of the map pool.
========================== ========= =================================================================================

The map pool is read from the file named in the cvar ``sv_mappoolfile``, looked up below ``baseq3`` in ``fs_homepath``
and then ``fs_basepath``. Each line has the form ``map|factory``, lines starting with ``#`` are ignored.

Compared to the Python plugin, every command that acts on a player also takes names. ``!allready``, ``!abort`` and
``!teamsize`` tell that no game is running instead of failing, ``!random`` needs a positive limit and ``!lock`` with an
unknown team answers with ``Invalid team.``.

motd
====

Greets players who joined with the message of the day, and plays a sound. Every home path of a server has its own
message, so several servers can share a database.

=========================================================== ======= ======================================================
command                                                     level   description
=========================================================== ======= ======================================================
``!setmotd <motd>``, ``!newmotd``                           4       Sets the message of the day of this server.
``!addmotd <more_motd>``                                    4       Adds a line to the message of this server.
``!clearmotd``, ``!removemotd``                             4       Removes the message of this server.
``!getmotd``, ``!motd``                                     0       Shows the message of the day.
``!setmotdall <motd>``, ``!newmotdall``                     4       Sets the message of all servers.
``!addmotdall <more_motd>``                                 4       Adds a line to the message of all servers.
``!clearmotdall``, ``!removemotdall``                       4       Removes the message of all servers.
=========================================================== ======= ======================================================

.. list-table::
   :header-rows: 1

   * - cvar
     - default
     - description
   * - ``qlx_motdSound``
     - ``sound/vo/crash_new/37b_07_alt.wav``
     - The sound played with the message.
   * - ``qlx_motdHeader``
     - ``^6======= ^7Message of the Day ^6=======^7``
     - The header shown above the message.

names
=====

Lets players register a colored version of their Steam name.

========================= ======= ============================================================
command                   level   description
========================= ======= ============================================================
``!name <name>``          0       Sets the colored name. Also available via ``client_command``.
========================= ======= ============================================================

=========================== ========= =============================================================
cvar                        default   description
=========================== ========= =============================================================
``qlx_enforceSteamName``    ``1``     The colored name has to match the Steam name without colors.
=========================== ========= =============================================================

If the Steam name of a player is not known, the current name without colors is used instead of failing.

clan
====

Lets players choose their own clan tag.

========================= ======= ============================================================
command                   level   description
========================= ======= ============================================================
``!clan <tag>``           0       Sets the clan tag. Also available via ``client_command``.
========================= ======= ============================================================

An empty tag removes the clan tag.

log
===

Writes the chat and the commands of the players to ``chatlogs/chat.log`` in ``fs_homepath``, together with the
connects and disconnects. The log files are rotated.

=========================== ============= =====================================
cvar                        default       description
=========================== ============= =====================================
``qlx_chatlogs``            ``3``         Number of log files that are kept.
``qlx_chatlogsSize``        ``3000000``   Maximum size of a log file in bytes.
=========================== ============= =====================================

demos
=====

Lets players ask to keep the recording of a match, and admins see what is recorded. The recording itself is part of the
server (the Cargo feature ``demo-recording``, off by default and still in development, and the cvar
``qlx_demoRecord``), see :func:`request_demo`; without the feature the commands only say that the server cannot record
demos. The plugin is not among the default plugins of ``qlx_plugins``, load it with ``r!demos``.

========================== ======= ================================================================================
command                    level   description
========================== ======= ================================================================================
``!request demo [reason]`` 1       Asks to keep the recording of the running match, or in the warmup after a match
                                   that of this match.
``!demos [count]``         4       Lists the newest recordings (10 by default, at most 30) with their size and the
                                   reasons of the requests.
``!demostatus``            4       Shows what the recorder is doing and how much is stored.
========================== ======= ================================================================================

The commands are answered to the caller only and do not show up in the chat, so the others do not learn that a match
was reported. They can be used from the console with ``qlx !demostatus`` as well.

============================= ========= ==========================================================================
cvar                          default   description
============================= ========= ==========================================================================
``qlx_demoRequestPermission`` ``1``     Permission level for ``!request demo``, read when the plugin is loaded.
============================= ========= ==========================================================================

workshop
========

Adds the workshop items from the cvar ``qlx_workshopReferences`` to every map. The cvar takes a comma separated list
of workshop item ids. The items are also added to the map that is already running when the plugin is loaded, so that
the first players of a freshly started server get them too. Items that are already referenced are not added twice.

Installing and removing items while the server runs
---------------------------------------------------

``!workshop add <id>`` lets Steam download the item (the engine command ``steam_downloadugc``), mounts it at once, adds
it to the list of the server and scans the maps again, so ``map_info`` knows its maps. The reply tells when the item
has arrived; a large item can take a while. Another ``add`` for an item that is still downloading does not start the
download again and tells how far it is; other items can be added meanwhile, Steam then fetches the latest one first.
The plugin keeps waiting as long as Steam reports the item as queued or downloading, and reports a failed download at
once with the ``EResult`` code of Steam (``9``: there is no such item).

For an item that is on the disk already, ``add`` asks Steam for a newer version, which the engine itself never does,
and installs the item when Steam is done. If that fails, the version on the disk is installed as long as Steam still
regards it as complete.

``!workshop remove <id>`` takes the item out of the list; it stays mounted until the next restart of the server. It
refuses an item that is still downloading and one that the running game uses (the running map comes from it, or
``qlx_workshopReferences`` names it), because players could not connect any more once the files they are checked against
are gone. ``!workshop list`` summarises the list and names the items whose files are missing; ``!workshop list all``
names every item.

The list is the ``workshop.txt`` of the server (the cvar ``sv_workshopFile`` of the engine). The engine reads the one
in the home path and only falls back to the base path when there is none; it does not merge them. The plugin therefore
always writes the list in the home path, and copies the list of the base path into it with the first change, so that
servers that share a base path keep their own lists. A line holds the item id and a comment, which the plugin writes as
``<id> # <pk3-name> (<map>, <map>, ...), <pk3-name> (...)``; comments, blank lines and the order are kept.

By default the files of a removed item stay on the disk, because the content directory of Steam is shared by all
servers that use the same installation. ``!workshop remove <id> purge`` also deletes the directory of the item and its
entries in the manifest of Steam (``appworkshop_282440.acf``) and takes it out of the search path of the server; a later
``add`` downloads it again, no restart is needed. ``keep`` forces the default when ``qlx_workshopPurge`` is set to
``1``.

.. warning::
   The plugin only knows whether its own server uses an item. A server on which another one deletes a mounted item stops
   when it loads one of its maps, and players cannot connect to it while it uses the item. Only purge where no other
   server of the installation has the item.

What the engine can take is limited by the number of ``.pk3`` files, not by the number of workshop items. Measured on a
test server with the engine alone, the names and checksums of all ``.pk3`` files go into info strings of 8192
characters, which takes the length of the name plus about 14 characters per file, and from about 380 ``.pk3`` files
the engine reports ``Info string length exceeded`` after the start of a game. From about 750 to 790 ``.pk3`` files it
stops with ``Z_Malloc: failed on allocation ... from the main zone`` when a map is loaded; ``com_zoneMegs 64`` raised
that limit beyond 990. The values depend on the length of the names and the number of files in the archives. The cvar
``qlx_workshopWarnPk3`` names the number of ``.pk3`` files of the listed items above which ``add`` and ``list`` warn.

=========================== ============= =========================================================================
Cvar                        Default       Meaning
=========================== ============= =========================================================================
``qlx_workshopFile``        empty         Path of the list to use instead of the ``workshop.txt`` of the home path.
``qlx_workshopPurge``       ``0``         ``1`` deletes the files with every ``remove``.
``qlx_workshopWarnPk3``     ``350``       Number of ``.pk3`` files above which to warn; ``0`` does not warn.
``qlx_workshopPermission``  ``5``         Permission level for ``!workshop``.
=========================== ============= =========================================================================
