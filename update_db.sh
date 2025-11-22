rm -rf ./podcast_data/*
cargo run -- --import-opml-file db/gnulinux_podcast_sammlung.opml --headless 2> /dev/null
cargo run -- --import-opml-file db/castero-podcasts.opml.xml --headless 2> /dev/null
cargo run -- --import-opml-file db/google-podcasts-subscriptions.opml.xml --headless 2> /dev/null
