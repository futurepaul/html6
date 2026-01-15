# html6

A native renderer for Hypernote. Uses the [Masonry](https://github.com/linebender/xilem) widget toolkit to render the UI. But I'm a zig fanboy so I'm rewriting it in zig: [zello](https://github.com/futurepaul/zello)

<img width="912" height="940" alt="Screenshot 2026-01-15 at 10 27 19 AM" src="https://github.com/user-attachments/assets/0deb60b0-124b-4eb0-8674-58bcd27f8f26" />

## Usage

There are Hypernote apps in the `apps` directory. You can run them with:

```bash
cargo run -- apps/hello.hnmd
```

`hello.hnmd` is defined like this:

```md
---
state:
  appName: "HNMD Demo"
  version: "0.1.0"
  count: 77 
  message: "Hello from Phase 3!"
---

# {state.appName}

Testing layout for a **Nostr-style feed**!

**Version:** {state.version}

**Count:** {state.count}

**Message:** {state.message}

new line!

yo

---

<hstack>
![Avatar](apps/waffle_dog.jpeg)
<vstack flex="1">
**Derek Ross** - *Oct 4, 2025, 10:21 AM*

Good morning and pura vida, Nostr! It's time to create notes and send zaps! 💜👥👍

Rewarding value is in itself valuable.

</vstack>

</hstack>

---

<hstack>
![Avatar](apps/waffle_dog.jpeg)
<vstack flex="1">
**Sebastix** - *Oct 4, 2025, 10:16 AM*

New Nostr stats 👀

🙏

Check out [plunder.tech](https://plunder.tech) for more info!

</vstack>
</hstack>

---

## Interactive Components

<button label="Click Me" />
<input name="message" placeholder="Type something..." />
```

