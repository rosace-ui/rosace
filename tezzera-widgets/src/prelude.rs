//! Convenience re-exports for all built-in widgets.
//!
//! Add `use tezzera_widgets::prelude::*;` to bring every widget into scope.

pub use crate::{
    Widget, PaintCtx, BoxedWidget, WidgetApp,
    TextAlign, FontWeight,
    AppBar, Avatar, Badge, Button, ButtonVariant,
    Card, Center, Checkbox, Chip, ColoredBox, Column, Container,
    Divider, EdgeInsets, Expanded, Icon, IconKind,
    ListTile, ListView, NavItem, NavRail, Padding, ProgressBar,
    RectReader,
    Row, Scaffold, ScrollView, SizedBox, Slider, Spacer, Stack, Switch,
    Tab, TabBar, Text, TextInput, Tooltip,
    Image, ImageCache, ImageFit, ImageSource, ImageWidget,
};
