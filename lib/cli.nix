{
  foldlAttrs,
  manifest,
  rustCli,
  writeShellScriptBin,
  works,
}:
let
  statusFns = foldlAttrs (
    acc: name: works:
    acc
    + ''
      function status-${name}() {
        ${works.status}/bin/status
      }
    ''
  ) "" works;
  statusAll =
    ''
      function status-all() {  
    ''
    + foldlAttrs (
      acc: name: works:
      acc + "${works.status}/bin/status\n"
    ) "" works
    + ''}'';
  startAll =
    ''
      function start-all() {
    ''
    + foldlAttrs (
      acc: name: works:
      acc + "${works.runner}/bin/run\n"
    ) "" works
    + ''}'';
  startFns = foldlAttrs (
    acc: name: works:
    acc
    + ''
      function start-${name}() {
        ${works.runner}/bin/run
      }
    ''
  ) "" works;
  stopAll =
    ''
      function stop-all() {
    ''
    + foldlAttrs (
      acc: name: works:
      acc + "${works.cleaner}/bin/clean\n"
    ) "" works
    + ''
      }
    '';
  stopFns = foldlAttrs (
    acc: name: works:
    acc
    + ''
      function stop-${name}() {
        ${works.cleaner}/bin/clean
      }
    ''
  ) "" works;
  restartFns = foldlAttrs (
    acc: name: _:
    acc
    + ''
      function restart-${name} {
        stop-${name}
        start-${name}      
      }
    ''
  ) "" works;
  names = foldlAttrs (
    acc: name: _:
    acc ++ [ name ]
  ) [ ] works;
  mkCmd = desc: fn: synonyms: {
    inherit desc fn synonyms;
  };
  actions = [
    (mkCmd "start service" "start" [
      "run"
      "r"
      "up"
    ])
    (mkCmd "stop service" "stop" [
      "s"
      "clean"
      "et-tu"
      "down"
    ])
    (mkCmd "restart a service" "restart" [
      "qq"
      "re"
    ])
    (mkCmd "show service status" "status" [
      "stat"
      "check"
      "ch"
    ])
    (mkCmd "inspect generated manifest" "inspect" [
      "show"
      "expose"
    ])
    (mkCmd "watch service dashboard" "tui" [
      "top"
    ])
  ];
  actionHelp = builtins.concatStringsSep "\n" (
    map (cmd: ''
      \t${cmd.fn}\t\tsynonyms: ${builtins.concatStringsSep " " cmd.synonyms}
      \t- ${cmd.desc}
    '') actions
  );
  help = ''
    [ides]: use "ides [action] [target]" to control services.
    actions: 
    ${actionHelp}

    \ttargets           synonyms: t
    \t- print a list of available targets

    \thelp
    \t- print this helpful information

    target names are the same as the attribute name used to define a service.
    eg. value of service.*.name, or serviceDefs.{name}

    an empty target will execute the action on all available services.
  '';
in
writeShellScriptBin "ides" ''
  targets=(${builtins.concatStringsSep " " names})

  function print-help() {
    printf '${help}'
  }

  function list-targets() {
    echo ''${targets[@]}
  }

  function check-target() {
    found=1
    for target in "''${targets[@]}"; do
      if [ "$1" == "$target" ]; then
        found=0
        break
      fi
    done
    printf $found
  }

  ${statusFns}

  ${statusAll}

  ${startFns}

  ${startAll}

  ${stopFns}

  ${stopAll}

  ${restartFns}

  function restart-all() {
    stop-all
    start-all
  }

  function action() {
    action=$1
    if [[ $# -gt 1 ]]; then
      shift
      for service in "$@"; do
        if [[ $(check-target $service) -eq 0 ]]; then
          $action-$service
        else
          echo "[ides]: no such target: $service"
        fi
      done
    else
      $action-all
    fi
  }

  case $1 in 
    run|r|up|start)
      shift
      ${rustCli}/bin/ides up --manifest ${manifest} "$@"
    ;;
    stop|s|clean|et-tu|down)
      shift
      ${rustCli}/bin/ides down --manifest ${manifest} "$@"
    ;;
    restart|qq|re)
      shift
      ${rustCli}/bin/ides restart --manifest ${manifest} "$@"
    ;;
    status|stat|check|ch)
      shift
      ${rustCli}/bin/ides status --manifest ${manifest} "$@"
    ;;
    inspect|show|expose)
      shift
      ${rustCli}/bin/ides inspect --manifest ${manifest} "$@"
    ;;
    tui|top)
      shift
      ${rustCli}/bin/ides tui --manifest ${manifest} "$@"
    ;;
    enter)
      shift
      ${rustCli}/bin/ides enter --manifest ${manifest} "$@"
    ;;
    leave|heartbeat)
      command=$1
      shift
      ${rustCli}/bin/ides "$command" "$@"
    ;;
    targets|t)
      list-targets
    ;;
    -h|h|help|*)
      print-help
    ;;
  esac
''
