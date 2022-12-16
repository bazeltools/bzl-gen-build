package io.bazeltools.buildgen.shared

import org.scalacheck.{Arbitrary, Gen}
import org.scalacheck.Prop.forAll

import Arbitrary.arbitrary

class PathTreeTests extends munit.ScalaCheckSuite {

  property("empty has nothing") {
    forAll { (ks: List[Byte]) =>
      assertEquals(
        PathTree.empty[Byte].pathGet(ks).toList,
        List.fill(ks.length + 1)(None)
      )
    }
  }

  def genPathTree[K: Ordering, V](
      gk: Gen[K],
      gv: Gen[V]
  ): Gen[PathTree[K, V]] = {
    val genKs = Gen.listOf(gk)
    Gen
      .listOf(Gen.zip(genKs, Gen.option(gv)))
      .map { kvs =>
        kvs.foldLeft(PathTree.empty[K]: PathTree[K, V]) { case (t, (k, v)) =>
          t.updated(k, v)
        }
      }
  }

  implicit def arbTree[K: Ordering: Arbitrary, V: Arbitrary]
      : Arbitrary[PathTree[K, V]] =
    Arbitrary(genPathTree[K, V](arbitrary[K], arbitrary[V]))

  test("empty.isEmpty") {
    assert(PathTree.empty[Byte].isEmpty)
  }

  property("if we add something, the tree isn't empty") {
    forAll { (tree: PathTree[Byte, Int], ks: List[Byte], v: Int) =>
      val pt0 = tree.updated(ks, Some(v))
      assert(!pt0.isEmpty)
    }
  }

  property("if pathtree is empty, all gets are empty") {
    forAll { (tree: PathTree[Byte, Int], ks: List[Byte]) =>
      if (tree.isEmpty) assert(tree.getAt(ks).isEmpty)
      else {
        val notEmpty = tree.keys.map { k => tree.getAt(k) }.collectFirst {
          case Some(v) => v
        }
        assert(notEmpty.nonEmpty)
      }
    }
  }

  property("after update, get returns it") {
    forAll { (tree: PathTree[Byte, Int], ks: List[Byte], v: Option[Int]) =>
      val pt0 = tree.updated(ks, v)
      assertEquals(pt0.pathGet(ks).last, v)
    }
  }

  property("after updateSubTree, getSubTree returns it") {
    forAll {
      (tree: PathTree[Byte, Int], ks: List[Byte], v: PathTree[Byte, Int]) =>
        val pt0 = tree.updateSubTree(ks, v)
        assertEquals(pt0.getSubTree(ks), v)
    }
  }

  property("pathGet law") {
    forAll { (tree: PathTree[Byte, Int], ks: List[Byte]) =>
      val left = tree.pathGet(ks).toList
      val right = ks.inits.toList.reverse.map(tree.getSubTree(_).value)
      assertEquals(left, right)
    }
  }

  property("mostSpecific agrees with mostSpecificMapFilter") {
    forAll { (tree: PathTree[Byte, Int], ks: List[Byte]) =>
      val left = tree.mostSpecific(ks)
      val right = tree.mostSpecificMapFilter(ks) { (_, v) => Some(v) }
      assertEquals(left, right)
    }
  }

  property("mostSpecificMapFilter gets the right key") {
    forAll { (tree: PathTree[Byte, Int], ks: List[Byte]) =>
      val left = tree.mostSpecificMapFilter(ks) { (k, v) => Some((k, v)) }
      val right = left.flatMap { case (k, _) =>
        tree.getAt(k)
      }
      assertEquals(left.map(_._2), right)
    }
  }

  property("mostSpecific is most specific") {
    forAll { (tree: PathTree[Byte, Int], ks: List[Byte]) =>
      if (ks.isEmpty) {
        assertEquals(tree.mostSpecific(ks), tree.value)
      } else {
        val left = tree.mostSpecific(ks)
        val leftInit = tree.mostSpecific(ks.init)
        // either they are the same, or left is getAt(ks)
        assert((left == leftInit) || (left == tree.getAt(ks)))
      }
    }
  }

  property("after update, value does not change unless key is empty") {
    forAll { (tree: PathTree[Byte, Int], ks: List[Byte], v: Option[Int]) =>
      val pt0 = tree.updated(ks, v)
      if (pt0.isEmpty) {
        assert(v.isEmpty)
      }
      if (ks.nonEmpty) {
        assertEquals(pt0.pathGet(ks).head, tree.value)
      } else {
        assertEquals(pt0.pathGet(ks).head, v)
      }
    }
  }

  property("pathGet for all children has length at least 2") {
    forAll { (tree: PathTree[Byte, Int]) =>
      tree.children.foreach { k =>
        assertEquals(tree.pathGet(k :: Nil).length, 2)
      }
    }
  }

  property("pathToList and listToPath are inverses") {
    val genList = Gen.listOf(Gen.oneOf("foo", "bar", "baz"))
    forAll(genList) { lst =>
      val path = PathTree.listToPath(lst)
      val lst1 = PathTree.pathToList(path)
      assertEquals(lst1, lst)
    }
  }

  property("listToPath :+ / resolve iso") {
    val genPart = Gen.oneOf("foo", "bar", "baz")
    val genList = Gen.listOf(genPart)
    forAll(genList, genPart) { (lst, part) =>
      val path0 = PathTree.listToPath(lst).resolve(part)
      val path1 = PathTree.listToPath(lst :+ part)
      assertEquals(path0, path1)
    }
  }

  property("tree.keys.foldLeft(empty)(update) == tree") {
    forAll { (tree: PathTree[Byte, Int]) =>
      val t1 = tree.keys.foldLeft(PathTree.empty[Byte]: PathTree[Byte, Int]) {
        (t0, key) =>
          t0.updated(key, tree.getAt(key))
      }
      assertEquals(t1, tree)
    }
  }

  property("tree.toLazyList.fold(empty)(add) == tree") {
    forAll { (tree: PathTree[Byte, Int]) =>
      val t1 =
        tree.toLazyList.foldLeft(PathTree.empty[Byte]: PathTree[Byte, Int]) {
          case (t0, (key, v)) =>
            t0.updated(key, Some(v))
        }
      assertEquals(t1, tree)
    }
  }

  property("tree.getSubTree consistency") {
    forAll { (tree: PathTree[Boolean, Int], key: List[Boolean]) =>
      val subtree = tree.getSubTree(key)
      subtree.keys.foreach { k =>
        assertEquals(subtree.getAt(k), tree.getAt(key ::: k))
      }
    }
  }
}
