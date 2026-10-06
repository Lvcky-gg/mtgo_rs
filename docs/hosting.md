# Play with friends

The game runs on the host's computer. You can play on the same network or use
ngrok to let a friend connect over the internet. You do not need to enter your
computer's address or change your router settings.

## On the same network

1. The host opens **Host a game**, chooses a deck and **Nearby players**, and
   clicks **Host**.
2. The other player opens **Join a game** and chooses their deck.
3. Under **Nearby games**, click **Join** beside the host's game name.

Both computers must be on the same local network. If the game does not appear,
the host can use **Copy** to send the invite, and the guest can paste it into
**Invite link** and click **Join**. Guest Wi-Fi networks sometimes prevent
computers from connecting to each other; use online hosting in that case.

## Over the internet: first-time setup

Only the **host** needs an ngrok account. The guest needs only the game and the
invite link. Ngrok is built into the game; you do **not** need to download its
app, run terminal commands, or forward a port.

1. [Create a free ngrok account](https://dashboard.ngrok.com/signup) and complete
   any account verification ngrok asks for.
2. Open [Your Authtoken](https://dashboard.ngrok.com/get-started/your-authtoken)
   in the ngrok dashboard and copy the token. This is your **authtoken**, not an
   API key.
3. In the game, open **Host a game** and choose **Friends online**.
4. Paste the token into **Ngrok authtoken**. The field hides its contents.
5. Optionally check **Remember token on this device** to skip this step next
   time. Leave it unchecked on a shared computer.

Keep the token private. Send your friend the game's **invite link**, never your
token. If you choose to remember it, the game saves it in its local database;
it is not encrypted at rest. **Forget token** removes that saved copy.

The game uses an HTTPS tunnel, which the [ngrok free plan supports](https://ngrok.com/docs/pricing-limits/free-plan-limits).
Ngrok's account and usage limits still apply. This setup does not use the raw
TCP tunnel option that requires credit-card verification.

## Host and join an online game

1. The host chooses a deck, format, and match length, selects **Friends online**,
   and clicks **Host**.
2. Wait while the game opens the tunnel. When the invite appears, click **Copy**
   and send the link to your friend.
3. The guest opens **Join a game**, chooses a deck, pastes the link into
   **Invite link**, and clicks **Join**.
4. Keep the host's game open while you play. The tunnel closes automatically
   when the hosted match ends or you leave it.

An invite is valid for one hour and one guest. Host a new game to get a new
invite if it expires or a guest disconnects after joining. Both players should
use the updated version of the game: older versions cannot read tunnel invites.

Nearby discovery also stays available while an online game waits for a guest.
The guest never needs to install ngrok or enter the host's public address.

## If something goes wrong

| What you see | What to do |
| --- | --- |
| The game asks for a token | Copy **Your Authtoken** from the ngrok dashboard and paste it in hosting setup. |
| Could not connect to ngrok | Check your internet connection, token, and account verification. A revoked token must be replaced. |
| Ngrok could not open a tunnel | Close another tunnel using this account, check the account's usage limits in the dashboard, and try again. |
| Internet hosting timed out | Check your internet connection and retry. You can leave while it is connecting. |
| The invite cannot be used | Ask the host to start again and send a fresh invite. |
| Could not reach the host | Check that the host still has the game open and that both computers have internet access. |
| Your internet tunnel stopped | Check the host's connection and host again. Reconnecting to an interrupted match is not supported yet. |
| No nearby games appear | Try the invite link, use the same Wi-Fi network, or choose **Friends online**. |

Your firewall may ask whether to allow the game to receive local connections.
Allow it on your private network to use nearby hosting. Online hosting connects
outward to ngrok and forwards only to the game's chosen local port.

The game still verifies the opponent's identity and encrypts game messages end
to end through the tunnel. The token is not part of the invite and is never
sent to the guest.
