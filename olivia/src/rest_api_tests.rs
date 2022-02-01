#[macro_export]
#[doc(hidden)]
macro_rules! run_rest_api_tests {
    (
        oracle => $oracle:ident,
        routes => $routes:ident,
        curve => $curve:ty,
     { $($init:tt)* }) => {
        #[cfg(test)]
        #[allow(redundant_semicolons)]
        mod rest_api_tests {
            use super::*;
            use crate::rest_api::ErrorMessage;
            use warp::http;
            use serde_json::from_slice as j;
            use olivia_core::{ GetPath, http::*};
            use core::str::FromStr;

            #[tokio::test]
            async fn get_path() {
                $($init)*;
                let event_id = EventId::from_str("/test/one/two/3.occur").unwrap();

                let node = event_id.path();

                {
                    let res = warp::test::request()
                        .path(event_id.as_str())
                        .reply(&$routes)
                        .await;

                    dbg!(&event_id, res.body());

                    let error = j::<ErrorMessage>(&res.body()).expect("returns an error body");
                    assert_eq!(
                        error.error,
                        http::StatusCode::NOT_FOUND.canonical_reason().unwrap()
                    );
                    assert_eq!(res.status(), http::StatusCode::NOT_FOUND);
                }
                $oracle.add_event(event_id.clone().into()).await.unwrap();

                assert_eq!(
                    warp::test::request()
                        .path("/test/one/two/42")
                        .reply(&$routes)
                        .await
                        .status(),
                    http::StatusCode::NOT_FOUND,
                    "similar but non-existing path should 404"
                );

                for path in &[format!("{}", node), format!("{}/", node)] {
                    let res = warp::test::request().path(path).reply(&$routes).await;
                    dbg!(&path, &res.body());

                    assert_eq!(res.status(), 200);
                    let body = j::<GetPath>(&res.body()).unwrap();
                    assert_eq!(body.events, [event_id.event_kind()]);
                }

                $oracle
                    .add_event(EventId::from_str("/test/one/two/4.occur").unwrap().into())
                    .await
                    .unwrap();

                let res = warp::test::request()
                    .path(&format!("{}", node.parent().unwrap()))
                    .reply(&$routes)
                    .await;

                let body = j::<GetPath>(&res.body()).unwrap();
                assert_eq!(
                    body.child_desc,
                    ChildDesc::List {
                        list: vec![Child { name: "3".into(), kind: NodeKind::List }, Child { name: "4".into(), kind: NodeKind::List }]
                    }
                );
            }

            #[tokio::test]
            async fn get_root() {
                $($init)*;
                $oracle
                    .add_event(
                        EventId::from_str("/test/one/two/three.occur")
                            .unwrap()
                            .into(),
                    )
                    .await
                    .unwrap();

                let res = warp::test::request().path("/").reply(&$routes).await;
                assert_eq!(res.status(), 200);
                let body = j::<RootResponse<_>>(&res.body()).unwrap();
                assert_eq!(
                    body.node.child_desc,
                    ChildDesc::List {
                        list: vec![Child {
                            name: "test".into(),
                            kind: NodeKind::List,
                        }]
                    }
                );
                assert_eq!(body.public_keys, $oracle.public_keys());
            }

            #[tokio::test]
            async fn get_event(){
                $($init)*;
                let event_id = EventId::from_str("/test/one/two/three.occur").unwrap();

                $oracle
                    .add_event(event_id.clone().clone().into())
                    .await
                    .unwrap();

                assert_eq!(
                    warp::test::request()
                        .path("/test/one/two/four.occur")
                        .reply(&$routes)
                        .await
                        .status(),
                    http::StatusCode::NOT_FOUND,
                    "similar but non-existing event should 404"
                );

                let public_keys = {
                    let root = warp::test::request().path("/").reply(&$routes).await;
                    j::<RootResponse<$curve>>(&root.body())
                        .unwrap()
                        .public_keys
                };

                let res = warp::test::request()
                    .path(event_id.as_str())
                    .reply(&$routes)
                    .await;

                let body = j::<EventResponse<$curve>>(&res.body()).unwrap();

                assert!(body
                        .announcement
                        .verify_against_id(&event_id, &public_keys.announcement)
                        .is_some())
            }

            #[tokio::test]
            async fn get_event_with_param(){
                $($init)*;
                let event_id = EventId::from_str("/test/one/two/three.price?n=20").unwrap();

                $oracle
                    .add_event(event_id.clone().clone().into())
                    .await
                    .unwrap();

                let public_keys = {
                    let root = warp::test::request().path("/").reply(&$routes).await;
                    j::<RootResponse<$curve>>(&root.body())
                        .unwrap()
                        .public_keys
                };

                let res = warp::test::request()
                    .path(event_id.as_str())
                    .reply(&$routes)
                    .await;

                let body = j::<EventResponse<$curve>>(&res.body()).unwrap();

                assert!(body
                        .announcement
                        .verify_against_id(&event_id, &public_keys.announcement)
                        .is_some())
            }

            #[tokio::test]
            async fn get_gt_event(){
                $($init)*;
                use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
                /// https://url.spec.whatwg.org/#fragment-percent-encode-set
                const FRAGMENT: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'<').add(b'>').add(b'`');
                let event_id = EventId::from_str("/test/one/two/three.price＞1000").unwrap();

                let percent_encoded = utf8_percent_encode(event_id.as_str(), FRAGMENT).to_string();
                $oracle
                    .add_event(event_id.clone().clone().into())
                    .await
                    .unwrap();

                let public_keys = {
                    let root = warp::test::request().path("/").reply(&$routes).await;
                    j::<RootResponse<$curve>>(&root.body())
                        .unwrap()
                        .public_keys
                };

                let res = warp::test::request()
                    .path(&percent_encoded)
                    .reply(&$routes)
                    .await;

                dbg!(&percent_encoded, res.body());

                let body = j::<EventResponse<$curve>>(&res.body()).unwrap();

                assert!(body
                        .announcement
                        .verify_against_id(&event_id, &public_keys.announcement)
                        .is_some())
            }
        }
    }
}
