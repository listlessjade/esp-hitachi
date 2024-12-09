from textual import on
from textual.app import App, ComposeResult
from textual.containers import Vertical, VerticalGroup, Center
from textual.widgets import Label, Tree

from textual_slider import Slider


class SliderWithStepApp(App):
    CSS = """
    Screen {
        layout: grid;
        grid-size: 2 2;
        width: auto;
    }

    Vertical {
        height: auto;
    }

    .title {
        text-style: bold;
    }

    .box {
      height: 100%;
      width: auto;
      border: solid green;
    }

    #slider-box {
        width: auto;
        max-width: 50w;
    }

    """

    def compose(self) -> ComposeResult:
        with Vertical(classes="box"):
            tree: Tree[str] = Tree("system status")
            tree.root.expand()
            connect = tree.root.add("Connectivity", expand=True)
            connect.add_leaf("MAC: 2549034")
            connect.add_leaf("IP: 127.0.0.1")
            mdns = connect.add("MDNS")
            mdns.add_leaf("Hostname: aa")
            mdns.add_leaf("service type: asfvfds")
            mdns.add_leaf("service name: nmersn")
            yield tree

        with Vertical(classes="box"):
            yield Center(Label("Hitachi Strength: 0%", classes="title", id="force-slider-val"))
            yield Center(Slider(min=0, max=100, step=5, id="force-slider"))

    def on_mount(self) -> None:
        slider1 = self.query_one("#force-slider", Slider)
        slider1_value_label = self.query_one("#force-slider-val", Label)
        slider1_value_label.update(f"Hitachi Strength: {slider1.value}%")


    @on(Slider.Changed, "#force-slider")
    def on_slider_changed_slider1(self, event: Slider.Changed) -> None:
        value_label = self.query_one("#force-slider-val", Label)
        value_label.update(f"Hitachi Strength: {event.value}%")


if __name__ == "__main__":
    app = SliderWithStepApp()
    app.run()
