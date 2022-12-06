package io.bazeltools.buildgen.shared

import cats.Applicative
import cats.data.NonEmptyList
import java.nio.file.{Path, Paths}
import scala.collection.immutable.{SortedSet, SortedMap}

import cats.syntax.all._

/** This data structure is a tree with a path of List[K], where each key List[K]
  * can have both an optional value, and children trees
  */
sealed trait PathTree[K, +V] {
  def map[U](fn: V => U): PathTree[K, U]
  def traverse[F[_]: Applicative, U](fn: V => F[U]): F[PathTree[K, U]]
  def ordering: Ordering[K]
  def children: SortedSet[K]
  // same as k.inits.reverse.map(getSubTree(_).value)
  def pathGet(k: List[K]): NonEmptyList[Option[V]]

  def mostSpecific(k: List[K]): Option[V] =
    pathGet(k).toList.reverse.collectFirst { case Some(v) => v }

  def mostSpecificMapFilter[U](
      k: List[K]
  )(fn: (List[K], V) => Option[U]): Option[U] = {
    val keys = k.inits
    val values = pathGet(k).toList.reverse

    keys
      .zip(values.iterator)
      .map { case (k, optV) => optV.flatMap(fn(k, _)) }
      .collectFirst { case Some(u) => u }
  }

  def getAt(k: List[K]): Option[V]
  def updated[V1 >: V](k: List[K], v: Option[V1]): PathTree[K, V1]
  def transform[V1 >: V](k: List[K])(
      fn: Option[V1] => Option[V1]
  ): PathTree[K, V1] =
    updated(k, fn(getAt(k)))

  def getSubTree(k: List[K]): PathTree[K, V]
  def updateSubTree[V1 >: V](k: List[K], that: PathTree[K, V1]): PathTree[K, V1]
  def value: Option[V]
}

object PathTree {
  private case class Node[K, V](
      value: Option[V],
      branches: SortedMap[K, PathTree[K, V]]
  )(implicit val ordering: Ordering[K])
      extends PathTree[K, V] {
    def children = branches.keySet

    def map[U](fn: V => U): PathTree[K, U] =
      Node(value.map(fn), branches.view.mapValues(_.map(fn)).to(SortedMap))
    def traverse[F[_]: Applicative, U](fn: V => F[U]): F[PathTree[K, U]] =
      (value.traverse(fn), branches.traverse(_.traverse(fn))).mapN(Node(_, _))

    def pathGet(k: List[K]): NonEmptyList[Option[V]] =
      k match {
        case Nil => NonEmptyList(value, Nil)
        case head :: tail =>
          val rest = branches.get(head) match {
            case Some(n) => n
            case None    => empty
          }

          value :: rest.pathGet(tail)
      }

    def getAt(k: List[K]): Option[V] =
      k match {
        case Nil => value
        case head :: tail =>
          branches.get(head) match {
            case Some(n) => n.getAt(tail)
            case None    => None
          }
      }

    def getSubTree(k: List[K]): PathTree[K, V] = {
      @annotation.tailrec
      def loop[V1 <: V](self: Node[K, V1], k: List[K]): PathTree[K, V] =
        k match {
          case Nil => self
          case head :: tail =>
            self.branches.get(head) match {
              case None => empty
              case Some(rest) =>
                loop(toNode(rest), tail)
            }
        }

      loop(this, k)
    }

    def updateSubTree[V1 >: V](
        k: List[K],
        that: PathTree[K, V1]
    ): PathTree[K, V1] =
      k match {
        case Nil => that
        case head :: tail =>
          val nextValue = branches.get(head) match {
            case None       => empty
            case Some(rest) => rest
          }
          val nextUpdated = nextValue.updateSubTree(tail, that)
          Node(value, branches.updated(head, nextUpdated))
      }

    def updated[V1 >: V](k: List[K], v: Option[V1]): PathTree[K, V1] =
      k match {
        case Nil => Node(v, branches)
        case head :: tail =>
          val child = branches.get(head) match {
            case Some(n) => n
            case None    => PathTree.empty[K]
          }
          Node(value, branches.updated(head, child.updated(tail, v)))
      }
  }

  @inline private def toNode[K, V](p: PathTree[K, V]): Node[K, _ <: V] =
    p match {
      case n @ Node(_, _) => n
    }

  def empty[K: Ordering]: PathTree[K, Nothing] =
    Node(None, SortedMap.empty)(implicitly[Ordering[K]])

  @annotation.tailrec
  private def pathToList(p: Path, acc: List[String]): List[String] =
    if (p eq null) Nil
    else {
      val parent = p.getParent
      val fn = p.getFileName().toString
      if (fn.isEmpty) acc
      else {
        val acc1 = fn :: acc
        if (parent eq null) acc1
        else {
          pathToList(parent, acc1)
        }
      }
    }

  def pathToList(p: Path): List[String] = pathToList(p, Nil)

  def listToPath(ls: List[String]): Path =
    ls match {
      case Nil       => Paths.get("")
      case h :: Nil  => Paths.get(h)
      case h :: tail => Paths.get(h).resolve(listToPath(tail))
    }

  def fromIterable[A, K: Ordering](
      its: Iterable[A]
  )(pathOf: A => List[K]): PathTree[K, A] =
    its.foldLeft(PathTree.empty[K]: PathTree[K, A]) { (pt, a) =>
      pt.updated(pathOf(a), Some(a))
    }
}
