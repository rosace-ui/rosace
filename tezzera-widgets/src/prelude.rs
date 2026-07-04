//! Convenience re-exports for all built-in widgets.
//!
//! Add `use tezzera_widgets::prelude::*;` to bring every widget into scope.

pub use crate::{
    Widget, Alignment, PaintCtx, BoxedWidget, WidgetApp,
    TextAlign, FontWeight,
    AppBar, Avatar, Badge, Button, ButtonVariant,
    Card, Checkbox, Chip, Column, Container, CustomPaint,
    AspectRatio, BoxShape, CircularProgress, Dropdown, Drawer, Expander, Dialog, Divider, EdgeInsets, Grid, Expanded, Icon, IconKind,
    ListTile, ListView, Menu, NavItem, NavRail, ProgressBar,
    AbsorbPointer, IgnorePointer, PressApi, Pressable, RectReader, RepaintBoundary, TransformLayer,
    Positioned, Radio, Row, Scaffold, SegmentedControl, ScrollView, ScrollAxis, Sheet, Slider, Spacer, Stack, Switch,
    Skeleton, Tab, TabBar, Wrap, Text, TextInput, Toast, ToastKind, Tooltip,
    Image, ImageCache, ImageFit, ImageSource, ImageWidget,
};
