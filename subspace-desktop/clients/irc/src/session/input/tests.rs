use super::{COMMANDS, Input, Parsed, args, parse_command, parse_input};

#[test]
fn plain_and_literal_text() {
    assert_eq!(parse_input("hello"), Ok(Input::Say("hello".into())));
    assert_eq!(
        parse_input("//not a command"),
        Ok(Input::Say("/not a command".into()))
    );
    assert_eq!(parse_input("a\nb\n"), Ok(Input::Say("a\nb".into())));
}

#[test]
fn messaging_commands() {
    assert_eq!(parse_input("/me waves"), Ok(Input::Me("waves".into())));
    assert_eq!(
        parse_input("/msg bob hi there"),
        Ok(Input::Msg {
            target: "bob".into(),
            text: "hi there".into()
        })
    );
    assert_eq!(
        parse_input("/NOTICE #c heads up"),
        Ok(Input::Notice {
            target: "#c".into(),
            text: "heads up".into()
        })
    );
    assert_eq!(
        parse_input("/query bob"),
        Ok(Input::Query {
            nick: "bob".into(),
            text: None
        })
    );
    assert!(parse_input("/msg bob").is_err());
    assert!(parse_input("/query").is_err());
}

#[test]
fn channel_commands() {
    assert_eq!(
        parse_input("/join rust key"),
        Ok(Input::Join {
            channel: "#rust".into(),
            key: Some("key".into())
        })
    );
    assert_eq!(
        parse_input("/part #c bye"),
        Ok(Input::Part {
            channel: Some("#c".into()),
            reason: "bye".into()
        })
    );
    assert_eq!(
        parse_input("/part see you"),
        Ok(Input::Part {
            channel: None,
            reason: "see you".into()
        })
    );
    assert_eq!(
        parse_input("/topic"),
        Ok(Input::Topic {
            channel: None,
            text: None
        })
    );
    assert_eq!(
        parse_input("/kick #c bob too loud"),
        Ok(Input::Kick {
            channel: Some("#c".into()),
            nick: "bob".into(),
            reason: "too loud".into()
        })
    );
    assert_eq!(
        parse_input("/invite bob #c"),
        Ok(Input::Invite {
            nick: "bob".into(),
            channel: Some("#c".into())
        })
    );
    assert_eq!(
        parse_input("/mode +o bob"),
        Ok(Input::Mode(vec!["+o".into(), "bob".into()]))
    );
    assert!(parse_input("/join").is_err());
    assert!(parse_input("/kick #c").is_err());
}

#[test]
fn other_commands() {
    assert_eq!(parse_input("/nick new"), Ok(Input::Nick("new".into())));
    assert_eq!(parse_input("/quit"), Ok(Input::Quit(String::new())));
    assert_eq!(
        parse_input("/whois bob"),
        Ok(Input::Raw("WHOIS bob".into()))
    );
    assert_eq!(parse_input("/away"), Ok(Input::Raw("AWAY".into())));
    assert_eq!(
        parse_input("/quote PING x"),
        Ok(Input::Raw("PING x".into()))
    );
    assert_eq!(parse_input("/list"), Ok(Input::Raw("LIST".into())));
    assert_eq!(parse_input("/cs op #c"), Ok(Input::Raw("CS op #c".into())));
    assert!(parse_input("/").is_err());
    assert_eq!(parse_input("/raw"), Err("Usage: /raw <line>".into()));
}

#[test]
fn every_listed_command_and_alias_is_parsed_here_not_sent_raw() {
    for command in COMMANDS {
        for name in std::iter::once(&command.name).chain(command.aliases) {
            let parsed = parse_command(args(&format!("{name} #c bob text")));
            assert!(
                matches!(parsed, Parsed::Known(Some(_))),
                "/{name} is listed, so it must be handled"
            );
        }
        assert!(command.usage().starts_with(&format!("/{} ", command.name)));
        assert!(!command.description.is_empty());
    }
    assert_eq!(
        parse_input("/j"),
        Err("Usage: /join <#channel> [key]".into()),
        "an alias reports its command's usage"
    );
}
