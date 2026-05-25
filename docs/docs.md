## auto

Whether to autostart ides services at devshell instantiation\.



*Type:*
boolean



*Default:*

```nix
true
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## monitor



Enable shell lease hooks that stop and clean services when the last shell lease exits\. Integer values are accepted for the old monitor API but currently behave like ` true `\.



*Type:*
boolean or signed integer



*Default:*

```nix
true
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs



Concrete service definitions, as per submodule options\.
Please put service-related options into ` options.services ` instead, and use this to implement those options\.



*Type:*
attribute set of (submodule)

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.argv



Structured argv to supply to the service binary\. Use ` { config = "name"; } ` to splice in a generated static or runtime config path\.



*Type:*
list of (string or absolute path or (submodule))



*Default:*

```nix
[ ]
```



*Example:*

```nix
[
  "run"
  "-c"
  {
    config = "main";
  }
  "--adapter"
  "caddyfile"
]
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.configs



Named generated static or runtime config files that can be referenced from ` argv ` with ` { config = "name"; } `\.



*Type:*
attribute set of (submodule)



*Default:*

```nix
{ }
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.configs\.\<name>\.content



Attributes that define your config values\.



*Type:*
null or (attribute set)



*Default:*

```nix
null
```



*Example:*

```nix
{
  this = "that";
}
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.configs\.\<name>\.ext



If your service config requires a file extension, set it here\. This overrides ` format `’s output path’\.



*Type:*
string



*Default:*

```nix
""
```



*Example:*

```nix
"json"
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.configs\.\<name>\.file



Path to config file\. This overrides all other values\.



*Type:*
null or absolute path



*Default:*

```nix
null
```



*Example:*

```nix
"./configs/my-config.ini"
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.configs\.\<name>\.format



Config output format\.
One of:
` java json yaml toml ini xml php `\.



*Type:*
null or one of “java”, “json”, “yaml”, “toml”, “ini”, “xml”, “php”



*Default:*

```nix
null
```



*Example:*

```nix
"json"
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.configs\.\<name>\.formatter



Serialisation/writer function to apply to ` content `\.
` format ` will auto-apply the correct format if the option value is valid\.
Should take ` path: attrs: ` and return a storepath\.



*Type:*
anything



*Default:*

```nix
null
```



*Example:*

```nix
"pkgs.formats.yaml {}.generate"
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.configs\.\<name>\.runtime



Runtime-rendered config template\. Use this when config content must refer to ephemeral ides runtime paths\.



*Type:*
null or (submodule)



*Default:*

```nix
null
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.configs\.\<name>\.runtime\.fileName



Relative file name to use under the service runtime config directory\. Defaults to the config name plus any configured extension\.



*Type:*
null or string



*Default:*

```nix
null
```



*Example:*

```nix
"redis.conf"
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.configs\.\<name>\.runtime\.parts



Typed template parts rendered by ides immediately before starting the service\.



*Type:*
list of (string or absolute path or (attribute set))



*Default:*

```nix
[ ]
```



*Example:*

```nix
[
  "dir "
  {
    runtimePath = "data";
  }
]
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.configs\.\<name>\.text



Plaintext configuration to use\.



*Type:*
string



*Default:*

```nix
""
```



*Example:*

```nix
''
  http://*:8080 {
    respond "hello"
  }
''
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.dependencies



Service dependency and ordering metadata handled by the ides runtime\.



*Type:*
submodule



*Default:*

```nix
{ }
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.dependencies\.after



Services that must be ordered before this service when both are started or stopped together\.



*Type:*
list of string



*Default:*

```nix
[ ]
```



*Example:*

```nix
[
  "postgres"
]
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.dependencies\.before



Services that must be ordered after this service when both are started or stopped together\.



*Type:*
list of string



*Default:*

```nix
[ ]
```



*Example:*

```nix
[
  "worker"
]
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.dependencies\.partOf



Services whose stop or restart operations should also stop or restart this service\.



*Type:*
list of string



*Default:*

```nix
[ ]
```



*Example:*

```nix
[
  "app"
]
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.dependencies\.requires



Services that must be started before this service\. Missing services are treated as configuration errors\.



*Type:*
list of string



*Default:*

```nix
[ ]
```



*Example:*

```nix
[
  "postgres"
]
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.dependencies\.wants



Services that should be started before this service\. In ides this is currently strict like ` requires `, because all referenced services must exist in the manifest\.



*Type:*
list of string



*Default:*

```nix
[ ]
```



*Example:*

```nix
[
  "redis"
]
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.exec



Alternative executable name to use from ` pkg `\.



*Type:*
string



*Default:*

```nix
""
```



*Example:*

```nix
"caddy"
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.path



List of path options for the unit (see ` man systemd.path `) - supplied as a list due to some options allowing duplicates\.



*Type:*
attribute set of list of string



*Default:*

```nix
{ }
```



*Example:*

```nix
{
  PathModified = [
    "/some/path"
  ];
}
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.pkg



Package to use for service\.



*Type:*
null or package



*Default:*

```nix
null
```



*Example:*

```nix
"pkgs.caddy"
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.runtime



Ephemeral runtime defaults for the service\.



*Type:*
submodule



*Default:*

```nix
{ }
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.runtime\.env



Additional environment variables to inject into the transient service\.



*Type:*
attribute set of string



*Default:*

```nix
{ }
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.runtime\.ephemeral



Place default writable paths under the ides runtime directory and clean them on explicit stop\.



*Type:*
boolean



*Default:*

```nix
true
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.shellCommand



Raw shell command to run for the service\. This is an escape hatch for services that do not fit package/executable/argv\.



*Type:*
null or string



*Default:*

```nix
null
```



*Example:*

```nix
"\${pkgs.lib.getExe pkgs.caddy} run -c \${pkgs.writeText \"Caddyfile\" cfg.extraConfig} --adapter caddyfile"
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.socket



List of socket options for the unit (see ` man systemd.socket `) - supplied as a list due to some options allowing duplicates\.



*Type:*
attribute set of list of string



*Default:*

```nix
{ }
```



*Example:*

```nix
{
  ListenStream = [
    "/run/user/1000/myapp.sock"
  ];
}
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.systemd



Small set of systemd service properties that ides can preserve when starting transient units\.



*Type:*
submodule



*Default:*

```nix
{ }
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.systemd\.execReload



Commands to run as systemd ExecReload entries\.



*Type:*
list of (string or (submodule))



*Default:*

```nix
[ ]
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.systemd\.execStartPost



Commands to run as systemd ExecStartPost entries after the main service command starts\.



*Type:*
list of (string or (submodule))



*Default:*

```nix
[ ]
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.systemd\.execStartPre



Commands to run as systemd ExecStartPre entries before the main service command\.



*Type:*
list of (string or (submodule))



*Default:*

```nix
[ ]
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.systemd\.execStop



Commands to run as systemd ExecStop entries when the transient unit stops\.



*Type:*
list of (string or (submodule))



*Default:*

```nix
[ ]
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.systemd\.ignoreStartFailure



Whether systemd should ignore the main ExecStart command’s failure\.



*Type:*
boolean



*Default:*

```nix
false
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.systemd\.killMode



systemd KillMode value to pass to the transient unit\.



*Type:*
null or string



*Default:*

```nix
null
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.systemd\.notifyAccess



systemd NotifyAccess value for notify-style services\. Defaults to ` all ` for notify services when omitted\.



*Type:*
null or string



*Default:*

```nix
null
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.systemd\.remainAfterExit



systemd RemainAfterExit value to pass to the transient unit\.



*Type:*
null or boolean



*Default:*

```nix
null
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.systemd\.restart



systemd Restart policy to pass to the transient unit\.



*Type:*
null or string



*Default:*

```nix
null
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.systemd\.restartSec



systemd RestartSec duration to pass to the transient unit\.



*Type:*
null or string



*Default:*

```nix
null
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.systemd\.serviceType



systemd service Type to pass to the transient unit, such as ` simple `, ` exec `, ` oneshot `, or ` notify `\.



*Type:*
null or string



*Default:*

```nix
null
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.systemd\.timeoutSec



systemd TimeoutSec duration to apply to transient service start and stop timeouts\.



*Type:*
null or string



*Default:*

```nix
null
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.systemd\.timeoutStartSec



systemd TimeoutStartSec duration to pass to the transient unit\.



*Type:*
null or string



*Default:*

```nix
null
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.systemd\.timeoutStopSec



systemd TimeoutStopSec duration to pass to the transient unit\.



*Type:*
null or string



*Default:*

```nix
null
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.systemd\.watchdogSec



systemd WatchdogSec duration to pass to the transient unit\.



*Type:*
null or string



*Default:*

```nix
null
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.systemd\.workingDirectory



Working directory to pass to the transient unit\. Defaults to the ides runtime data directory\.



*Type:*
null or string



*Default:*

```nix
null
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## serviceDefs\.\<name>\.timer



List of timer options for the unit (see ` man systemd.timer `) - supplied as a list due to some options allowing duplicates\.



*Type:*
attribute set of list of string



*Default:*

```nix
{ }
```



*Example:*

```nix
{
  OnActiveSec = [
    "50s"
  ];
}
```

*Declared by:*
 - [lib/options\.nix](https://git.atagen.co/atagen/ides/lib/options.nix)



## services\.redis\.enable



Whether to enable Enable Redis…



*Type:*
boolean



*Default:*

```nix
false
```



*Example:*

```nix
true
```

*Declared by:*
 - [modules/redis\.nix](https://git.atagen.co/atagen/ides/modules/redis.nix)



## services\.redis\.bind



List of IPs to bind to\.



*Type:*
list of string



*Default:*

```nix
[
  "127.0.0.1"
  "::1"
]
```

*Declared by:*
 - [modules/redis\.nix](https://git.atagen.co/atagen/ides/modules/redis.nix)



## services\.redis\.databases



Number of databases\.



*Type:*
signed integer



*Default:*

```nix
16
```

*Declared by:*
 - [modules/redis\.nix](https://git.atagen.co/atagen/ides/modules/redis.nix)



## services\.redis\.extraConfig



Additional config directives\.



*Type:*
string



*Default:*

```nix
""
```

*Declared by:*
 - [modules/redis\.nix](https://git.atagen.co/atagen/ides/modules/redis.nix)



## services\.redis\.logLevel



Logging verbosity level\.



*Type:*
one of “debug”, “verbose”, “notice”, “warning”, “nothing”



*Default:*

```nix
"notice"
```

*Declared by:*
 - [modules/redis\.nix](https://git.atagen.co/atagen/ides/modules/redis.nix)



## services\.redis\.name



The name ides uses for this service\.



*Type:*
string



*Default:*

```nix
"redis"
```

*Declared by:*
 - [modules/redis\.nix](https://git.atagen.co/atagen/ides/modules/redis.nix)



## services\.redis\.port



Port to bind to\.



*Type:*
integer between 1024 and 65535 (both inclusive)



*Default:*

```nix
6379
```

*Declared by:*
 - [modules/redis\.nix](https://git.atagen.co/atagen/ides/modules/redis.nix)



## services\.redis\.socket



Unix socket to bind to\. Relative paths are placed under the service runtime run directory; absolute paths are used as-is\.



*Type:*
null or string



*Default:*

```nix
null
```

*Declared by:*
 - [modules/redis\.nix](https://git.atagen.co/atagen/ides/modules/redis.nix)



## services\.redis\.socketPerms



Permissions for the unix socket\.



*Type:*
null or signed integer



*Default:*

```nix
null
```

*Declared by:*
 - [modules/redis\.nix](https://git.atagen.co/atagen/ides/modules/redis.nix)



## systemd\.services



Compatibility subset of NixOS ` systemd.services `\.

ides lowers simple foreground services into ` serviceDefs `\. Supported
fields are ` script `, single-command ` serviceConfig.ExecStart `, ` path `,
` environment `, a small transient-safe ` serviceConfig ` subset, and local
service dependency lists\. Target/unit installation fields such as
` wantedBy ` are accepted but ignored\.



*Type:*
attribute set of (submodule)



*Default:*

```nix
{ }
```

*Declared by:*
 - [lib/nixos-compat\.nix](https://git.atagen.co/atagen/ides/lib/nixos-compat.nix)



## systemd\.services\.\<name>\.enable



Whether to lower this NixOS-style systemd service into an ides service definition\.



*Type:*
boolean



*Default:*

```nix
true
```

*Declared by:*
 - [lib/nixos-compat\.nix](https://git.atagen.co/atagen/ides/lib/nixos-compat.nix)



## systemd\.services\.\<name>\.after



NixOS-style ordering constraints\. Local service references are lowered into ides dependencies\.



*Type:*
list of string



*Default:*

```nix
[ ]
```

*Declared by:*
 - [lib/nixos-compat\.nix](https://git.atagen.co/atagen/ides/lib/nixos-compat.nix)



## systemd\.services\.\<name>\.before



NixOS-style reverse ordering constraints\. Local service references are lowered into ides dependencies\.



*Type:*
list of string



*Default:*

```nix
[ ]
```

*Declared by:*
 - [lib/nixos-compat\.nix](https://git.atagen.co/atagen/ides/lib/nixos-compat.nix)



## systemd\.services\.\<name>\.description



Service description\. Kept for NixOS compatibility; currently not emitted into the ides manifest\.



*Type:*
string



*Default:*

```nix
""
```

*Declared by:*
 - [lib/nixos-compat\.nix](https://git.atagen.co/atagen/ides/lib/nixos-compat.nix)



## systemd\.services\.\<name>\.environment



Environment variables injected into the lowered ides service\.



*Type:*
attribute set of (string or absolute path or signed integer or boolean)



*Default:*

```nix
{ }
```

*Declared by:*
 - [lib/nixos-compat\.nix](https://git.atagen.co/atagen/ides/lib/nixos-compat.nix)



## systemd\.services\.\<name>\.partOf



NixOS-style PartOf constraints\. Local service references are lowered into ides dependencies\.



*Type:*
list of string



*Default:*

```nix
[ ]
```

*Declared by:*
 - [lib/nixos-compat\.nix](https://git.atagen.co/atagen/ides/lib/nixos-compat.nix)



## systemd\.services\.\<name>\.path



Packages or paths added to PATH for script-style services\.



*Type:*
list of (package or absolute path or string)



*Default:*

```nix
[ ]
```

*Declared by:*
 - [lib/nixos-compat\.nix](https://git.atagen.co/atagen/ides/lib/nixos-compat.nix)



## systemd\.services\.\<name>\.preStart



Shell script to run before the service command\.



*Type:*
strings concatenated with “\\n”



*Default:*

```nix
""
```

*Declared by:*
 - [lib/nixos-compat\.nix](https://git.atagen.co/atagen/ides/lib/nixos-compat.nix)



## systemd\.services\.\<name>\.requiredBy



Accepted for NixOS compatibility but ignored by ides\.



*Type:*
list of string



*Default:*

```nix
[ ]
```

*Declared by:*
 - [lib/nixos-compat\.nix](https://git.atagen.co/atagen/ides/lib/nixos-compat.nix)



## systemd\.services\.\<name>\.requires



NixOS-style service requirements\. Local service references are lowered into ides dependencies\.



*Type:*
list of string



*Default:*

```nix
[ ]
```

*Declared by:*
 - [lib/nixos-compat\.nix](https://git.atagen.co/atagen/ides/lib/nixos-compat.nix)



## systemd\.services\.\<name>\.script



Shell script to run as the service command\.



*Type:*
strings concatenated with “\\n”



*Default:*

```nix
""
```

*Declared by:*
 - [lib/nixos-compat\.nix](https://git.atagen.co/atagen/ides/lib/nixos-compat.nix)



## systemd\.services\.\<name>\.serviceConfig



Small supported subset of NixOS serviceConfig\. ides currently accepts command lifecycle hooks, Type, NotifyAccess, Restart, common timeout fields, RemainAfterExit, KillMode, and WorkingDirectory\.



*Type:*
attribute set of anything



*Default:*

```nix
{ }
```

*Declared by:*
 - [lib/nixos-compat\.nix](https://git.atagen.co/atagen/ides/lib/nixos-compat.nix)



## systemd\.services\.\<name>\.wantedBy



Accepted for NixOS compatibility but ignored by ides\.



*Type:*
list of string



*Default:*

```nix
[ ]
```

*Declared by:*
 - [lib/nixos-compat\.nix](https://git.atagen.co/atagen/ides/lib/nixos-compat.nix)



## systemd\.services\.\<name>\.wants



NixOS-style service wants\. Local service references are lowered into ides dependencies\.



*Type:*
list of string



*Default:*

```nix
[ ]
```

*Declared by:*
 - [lib/nixos-compat\.nix](https://git.atagen.co/atagen/ides/lib/nixos-compat.nix)


